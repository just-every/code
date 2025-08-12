use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use codex_core::codex_wrapper::CodexConversation;
use codex_core::codex_wrapper::init_codex;
use codex_core::config::Config;
use codex_core::config_types::ReasoningEffort;
use codex_core::parse_command::ParsedCommand;
use codex_core::protocol::AgentMessageDeltaEvent;
use codex_core::protocol::AgentMessageEvent;
use codex_core::protocol::AgentReasoningDeltaEvent;
use codex_core::protocol::AgentReasoningEvent;
use codex_core::protocol::AgentReasoningRawContentDeltaEvent;
use codex_core::protocol::AgentReasoningRawContentEvent;
use codex_core::protocol::ApplyPatchApprovalRequestEvent;
use codex_core::protocol::BackgroundEventEvent;
use codex_core::protocol::BrowserScreenshotUpdateEvent;
use codex_core::protocol::ErrorEvent;
use codex_core::protocol::Event;
use codex_core::protocol::EventMsg;
use codex_core::protocol::ExecApprovalRequestEvent;
use codex_core::protocol::ExecCommandBeginEvent;
use codex_core::protocol::ExecCommandEndEvent;
use codex_core::protocol::InputItem;
use codex_core::protocol::McpToolCallBeginEvent;
use codex_core::protocol::McpToolCallEndEvent;
use codex_core::protocol::Op;
use codex_core::protocol::PatchApplyBeginEvent;
use codex_core::protocol::TaskCompleteEvent;
use codex_core::protocol::TokenUsage;
use codex_core::protocol::TurnDiffEvent;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Widget;
use ratatui::widgets::WidgetRef;
use ratatui_image::picker::Picker;
use std::cell::RefCell;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::mpsc::unbounded_channel;
use tracing::info;
// use image::GenericImageView;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::BottomPane;
use crate::bottom_pane::BottomPaneParams;
use crate::bottom_pane::CancellationEvent;
use crate::bottom_pane::InputResult;
use crate::history_cell::CommandOutput;
use crate::history_cell::HistoryCell;
use crate::history_cell::PatchEventType;
use crate::live_wrap::RowBuilder;
use crate::text_block::TextBlock;
use crate::user_approval_widget::ApprovalRequest;
use codex_browser::{BrowserManager, config::BrowserConfig};
use codex_file_search::FileMatch;
use ratatui::style::Stylize;

struct RunningCommand {
    command: Vec<String>,
    parsed: Vec<ParsedCommand>,
}

pub(crate) struct ChatWidget<'a> {
    app_event_tx: AppEventSender,
    codex_op_tx: UnboundedSender<Op>,
    bottom_pane: BottomPane<'a>,
    active_history_cell: Option<HistoryCell>,
    history_cells: Vec<HistoryCell>,  // Store all history in memory
    config: Config,
    initial_user_message: Option<UserMessage>,
    total_token_usage: TokenUsage,
    last_token_usage: TokenUsage,
    reasoning_buffer: String,
    content_buffer: String,
    // Buffer for streaming assistant answer text; we do not surface partial
    // We wait for the final AgentMessage event and then emit the full text
    // at once into scrollback so the history contains a single message.
    answer_buffer: String,
    running_commands: HashMap<String, RunningCommand>,
    live_builder: RowBuilder,
    current_stream: Option<StreamKind>,
    stream_header_emitted: bool,
    live_max_rows: u16,
    // Store pending image paths keyed by their placeholder text
    pending_images: HashMap<String, PathBuf>,
    welcome_shown: bool,
    // Browser manager for screenshot functionality
    browser_manager: Arc<BrowserManager>,
    // Path to the latest browser screenshot and URL for display
    latest_browser_screenshot: Arc<Mutex<Option<(PathBuf, String)>>>,
    // Cached image protocol to avoid recreating every frame (path, area, protocol)
    cached_image_protocol: std::cell::RefCell<Option<(PathBuf, Rect, ratatui_image::protocol::Protocol)>>,
    // Cached picker to avoid recreating every frame
    cached_picker: std::cell::RefCell<Option<Picker>>,
    
    // Cached cell size (width,height) in pixels
    cached_cell_size: std::cell::OnceCell<(u16, u16)>,
    // Scroll offset from bottom (0 = at bottom, positive = scrolled up)
    scroll_offset: u16,
}

struct UserMessage {
    text: String,
    image_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamKind {
    Answer,
    Reasoning,
}

impl From<String> for UserMessage {
    fn from(text: String) -> Self {
        Self {
            text,
            image_paths: Vec::new(),
        }
    }
}

fn create_initial_user_message(text: String, image_paths: Vec<PathBuf>) -> Option<UserMessage> {
    if text.is_empty() && image_paths.is_empty() {
        None
    } else {
        Some(UserMessage { text, image_paths })
    }
}

impl ChatWidget<'_> {
    pub(crate) fn insert_str(&mut self, s: &str) {
        self.bottom_pane.insert_str(s);
    }
    
    fn parse_message_with_images(&mut self, text: String) -> UserMessage {
        use std::path::Path;
        
        // Common image extensions
        const IMAGE_EXTENSIONS: &[&str] = &[
            ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp", ".svg", ".ico", ".tiff", ".tif"
        ];
        
        let mut image_paths = Vec::new();
        let mut cleaned_text = text.clone();
        
        // First, handle [image: ...] placeholders from drag-and-drop
        let placeholder_regex = regex_lite::Regex::new(r"\[image: [^\]]+\]").unwrap();
        for mat in placeholder_regex.find_iter(&text) {
            let placeholder = mat.as_str();
            if let Some(path) = self.pending_images.remove(placeholder) {
                image_paths.push(path);
                // Remove the placeholder from the text
                cleaned_text = cleaned_text.replace(placeholder, "");
            }
        }
        
        // Then check for direct file paths in the text
        let words: Vec<String> = text.split_whitespace().map(String::from).collect();
        
        for word in &words {
            // Skip placeholders we already handled
            if word.starts_with("[image:") {
                continue;
            }
            
            // Check if this looks like an image path
            let is_image_path = IMAGE_EXTENSIONS.iter().any(|ext| word.to_lowercase().ends_with(ext));
            
            if is_image_path {
                let path = Path::new(word);
                
                // Check if it's a relative or absolute path that exists
                if path.exists() {
                    image_paths.push(path.to_path_buf());
                    // Remove the path from the text
                    cleaned_text = cleaned_text.replace(word, "");
                } else {
                    // Try with common relative paths
                    let potential_paths = vec![
                        PathBuf::from(word),
                        PathBuf::from("./").join(word),
                        std::env::current_dir().ok().map(|d| d.join(word)).unwrap_or_default(),
                    ];
                    
                    for potential_path in potential_paths {
                        if potential_path.exists() {
                            image_paths.push(potential_path);
                            cleaned_text = cleaned_text.replace(word, "");
                            break;
                        }
                    }
                }
            }
        }
        
        // Clean up extra whitespace
        cleaned_text = cleaned_text.split_whitespace().collect::<Vec<_>>().join(" ");
        
        UserMessage {
            text: cleaned_text,
            image_paths,
        }
    }
    
    fn interrupt_running_task(&mut self) {
        if self.bottom_pane.is_task_running() {
            self.active_history_cell = None;
            self.bottom_pane.clear_ctrl_c_quit_hint();
            self.submit_op(Op::Interrupt);
            self.bottom_pane.set_task_running(false);
            self.bottom_pane.clear_live_ring();
            self.live_builder = RowBuilder::new(self.live_builder.width());
            self.current_stream = None;
            self.stream_header_emitted = false;
            self.answer_buffer.clear();
            self.reasoning_buffer.clear();
            self.content_buffer.clear();
            self.request_redraw();
        }
    }
    fn layout_areas(&self, area: Rect) -> Vec<Rect> {
        // Check if browser is active and has a screenshot
        let has_browser_screenshot = self.latest_browser_screenshot.lock()
            .map(|lock| lock.is_some())
            .unwrap_or(false);
        
        let bottom_height = 6u16.max(self.bottom_pane.desired_height(area.width)).min(15);

        if has_browser_screenshot {
            // match HUD padding used in render_browser_hud()
            let horizontal_padding = 2u16;
            let padded_area = Rect {
                x: area.x + horizontal_padding,
                y: area.y,
                width: area.width.saturating_sub(horizontal_padding * 2),
                height: area.height,
            };

            // use the actual 50/50 split
            let [_left, right] = Layout::horizontal([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ]).areas::<2>(padded_area);

            // inner width of the Preview block (remove 1-char borders)
            let inner_cols = right.width.saturating_sub(2);

            // rows = cols * (3/4) * (cell_w/cell_h)
            let (cw, ch) = self.measured_font_size();
            let numer = (inner_cols as u32) * 3 * (cw as u32);
            let denom = 4 * (ch as u32);

            // use FLOOR to avoid over-estimating (which creates bottom slack)
            let inner_rows: u16 = (numer / denom) as u16;

            // add back the top/bottom borders of the Preview block
            let mut hud_height = inner_rows.saturating_add(2);

            // one-line tighten to kill residual rounding slack
            hud_height = hud_height.saturating_sub(1);

            // keep within vertical budget (status+bottom+≥1 row history)
            let available = area.height.saturating_sub(3 + bottom_height + 1);
            hud_height = hud_height.clamp(4, available);

            return Layout::vertical([
                Constraint::Length(3),
                Constraint::Length(hud_height),
                Constraint::Fill(1),
                Constraint::Length(bottom_height),
            ]).areas::<4>(area).to_vec();
        } else {
            // Status bar, history, bottom pane (no HUD)
            Layout::vertical([
                Constraint::Length(3), // Status bar with box border
                Constraint::Fill(1), // History takes all remaining space
                Constraint::Length(bottom_height),
            ])
            .areas::<3>(area)
            .to_vec()
        }
    }
    fn emit_stream_header(&mut self, kind: StreamKind) {
        use ratatui::text::Line as RLine;
        if self.stream_header_emitted {
            return;
        }
        let header = match kind {
            StreamKind::Reasoning => RLine::from("thinking".magenta().italic()),
            StreamKind::Answer => RLine::from("codex".magenta().bold()),
        };
        self.app_event_tx
            .send(AppEvent::InsertHistory(vec![header]));
        self.stream_header_emitted = true;
    }
    fn finalize_active_stream(&mut self) {
        if let Some(kind) = self.current_stream {
            self.finalize_stream(kind);
        }
    }
    pub(crate) fn new(
        config: Config,
        app_event_tx: AppEventSender,
        initial_prompt: Option<String>,
        initial_images: Vec<PathBuf>,
        enhanced_keys_supported: bool,
    ) -> Self {
        let (codex_op_tx, mut codex_op_rx) = unbounded_channel::<Op>();

        let app_event_tx_clone = app_event_tx.clone();
        // Create the Codex asynchronously so the UI loads as quickly as possible.
        let config_for_agent_loop = config.clone();
        tokio::spawn(async move {
            let CodexConversation {
                codex,
                session_configured,
                ..
            } = match init_codex(config_for_agent_loop).await {
                Ok(vals) => vals,
                Err(e) => {
                    // TODO: surface this error to the user.
                    tracing::error!("failed to initialize codex: {e}");
                    return;
                }
            };

            // Forward the captured `SessionInitialized` event that was consumed
            // inside `init_codex()` so it can be rendered in the UI.
            app_event_tx_clone.send(AppEvent::CodexEvent(session_configured.clone()));
            let codex = Arc::new(codex);
            let codex_clone = codex.clone();
            tokio::spawn(async move {
                while let Some(op) = codex_op_rx.recv().await {
                    let id = codex_clone.submit(op).await;
                    if let Err(e) = id {
                        tracing::error!("failed to submit op: {e}");
                    }
                }
            });

            while let Ok(event) = codex.next_event().await {
                app_event_tx_clone.send(AppEvent::CodexEvent(event));
            }
        });

        // Browser manager will be initialized lazily when needed
        // For now, create a placeholder that will be replaced on first use
        let browser_config = BrowserConfig::default();
        let browser_manager = Arc::new(BrowserManager::new(browser_config));
        
        // Add initial animated welcome message to history
        let mut history_cells = Vec::new();
        history_cells.push(HistoryCell::AnimatedWelcome {
            start_time: std::time::Instant::now(),
            completed: std::cell::Cell::new(false),
        });

        // Initialize image protocol for rendering screenshots
        
        Self {
            app_event_tx: app_event_tx.clone(),
            codex_op_tx,
            bottom_pane: BottomPane::new(BottomPaneParams {
                app_event_tx,
                has_input_focus: true,
                enhanced_keys_supported,
            }),
            active_history_cell: None,
            history_cells,
            config,
            initial_user_message: create_initial_user_message(
                initial_prompt.unwrap_or_default(),
                initial_images,
            ),
            total_token_usage: TokenUsage::default(),
            last_token_usage: TokenUsage::default(),
            reasoning_buffer: String::new(),
            content_buffer: String::new(),
            answer_buffer: String::new(),
            running_commands: HashMap::new(),
            live_builder: RowBuilder::new(80),
            current_stream: None,
            stream_header_emitted: false,
            live_max_rows: 3,
            pending_images: HashMap::new(),
            welcome_shown: false,
            browser_manager,
            latest_browser_screenshot: Arc::new(Mutex::new(None)),
            cached_image_protocol: RefCell::new(None),
            cached_picker: RefCell::new(None),
            cached_cell_size: std::cell::OnceCell::new(),
            scroll_offset: 0,
        }
    }

    pub(crate) fn handle_key_event(&mut self, key_event: KeyEvent) {
        if key_event.kind == KeyEventKind::Press {
            self.bottom_pane.clear_ctrl_c_quit_hint();
        }

        match self.bottom_pane.handle_key_event(key_event) {
            InputResult::Submitted(text) => {
                let user_message = self.parse_message_with_images(text);
                self.submit_user_message(user_message);
            }
            InputResult::None => {}
        }
    }

    pub(crate) fn handle_paste(&mut self, text: String) {
        // Check if the pasted text is a file path to an image
        let trimmed = text.trim();
        
        tracing::info!("Paste received: {:?}", trimmed);
        
        const IMAGE_EXTENSIONS: &[&str] = &[
            ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp", ".svg", ".ico", ".tiff", ".tif"
        ];
        
        // Check if it looks like a file path
        let is_likely_path = trimmed.starts_with("file://") 
            || trimmed.starts_with("/") 
            || trimmed.starts_with("~/")
            || trimmed.starts_with("./");
            
        if is_likely_path {
            // Remove escape backslashes that terminals add for special characters
            let unescaped = trimmed
                .replace("\\ ", " ")
                .replace("\\(", "(")
                .replace("\\)", ")");
            
            // Handle file:// URLs (common when dragging from Finder)
            let path_str = if unescaped.starts_with("file://") {
                // URL decode to handle spaces and special characters
                // Simple decoding for common cases (spaces as %20, etc.)
                unescaped.strip_prefix("file://")
                    .map(|s| s.replace("%20", " ")
                             .replace("%28", "(")
                             .replace("%29", ")")
                             .replace("%5B", "[")
                             .replace("%5D", "]")
                             .replace("%2C", ",")
                             .replace("%27", "'")
                             .replace("%26", "&")
                             .replace("%23", "#")
                             .replace("%40", "@")
                             .replace("%2B", "+")
                             .replace("%3D", "=")
                             .replace("%24", "$")
                             .replace("%21", "!")
                             .replace("%2D", "-")
                             .replace("%2E", "."))
                    .unwrap_or_else(|| unescaped.clone())
            } else {
                unescaped
            };
            
            tracing::info!("Decoded path: {:?}", path_str);
            
            // Check if it has an image extension
            let is_image = IMAGE_EXTENSIONS.iter().any(|ext| path_str.to_lowercase().ends_with(ext));
            
            if is_image {
                let path = PathBuf::from(&path_str);
                tracing::info!("Checking if path exists: {:?}", path);
                if path.exists() {
                    tracing::info!("Image file dropped/pasted: {:?}", path);
                    // Get just the filename for display
                    let filename = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("image");
                    
                    // Add a placeholder to the compose field instead of submitting
                    let placeholder = format!("[image: {}]", filename);
                    
                    // Store the image path for later submission
                    self.pending_images.insert(placeholder.clone(), path);
                    
                    // Add the placeholder text to the compose field
                    self.bottom_pane.handle_paste(placeholder);
                    return;
                } else {
                    tracing::warn!("Image path does not exist: {:?}", path);
                }
            }
        }
        
        // Otherwise handle as regular text paste
        self.bottom_pane.handle_paste(text);
    }

    fn add_to_history(&mut self, cell: HistoryCell) {
        // Store in memory instead of sending to scrollback
        self.history_cells.push(cell);
        // Auto-scroll to bottom when new content arrives
        self.scroll_offset = 0;
        // Check if there's actual conversation history (any user prompts submitted)
        let has_conversation = self.history_cells.iter().any(|cell| {
            matches!(cell, HistoryCell::UserPrompt { .. })
        });
        self.bottom_pane.set_has_chat_history(has_conversation);
        // Ensure input focus is maintained when new content arrives
        self.bottom_pane.ensure_input_focus();
        // Request redraw to show new history
        self.app_event_tx.send(AppEvent::RequestRedraw);
    }

    fn submit_user_message(&mut self, user_message: UserMessage) {
        let UserMessage { text, image_paths } = user_message;
        
        // Keep the original text for display purposes
        let original_text = text.clone();
        let mut actual_text = text;
        
        // Process slash commands and expand them if needed
        let processed = crate::slash_command::process_slash_command_message(&actual_text);
        match processed {
            crate::slash_command::ProcessedCommand::ExpandedPrompt(expanded) => {
                // Replace the slash command with the expanded prompt for the LLM
                actual_text = expanded;
            }
            crate::slash_command::ProcessedCommand::RegularCommand(cmd, _args) => {
                // This is a regular slash command, dispatch it normally
                self.app_event_tx.send(AppEvent::DispatchCommand(cmd, actual_text.clone()));
                return;
            }
            crate::slash_command::ProcessedCommand::Error(error_msg) => {
                // Show error in history
                self.add_to_history(HistoryCell::new_error_event(error_msg));
                return;
            }
            crate::slash_command::ProcessedCommand::NotCommand(_) => {
                // Not a slash command, process normally
            }
        }
        
        let mut items: Vec<InputItem> = Vec::new();
        let mut browser_screenshot_count = 0;

        // Check if browser mode is enabled and capture screenshot
        // Always check both local and global browser managers
        let local_browser_manager = self.browser_manager.clone();
        
        // Check if either local or global browser is enabled
        let local_enabled = local_browser_manager.config.try_read()
            .map(|c| c.enabled)
            .unwrap_or(false);
        
        // Check global browser manager async
        let (tx_global, rx_global) = std::sync::mpsc::channel();
        tokio::spawn(async move {
            let global_manager = codex_browser::global::get_browser_manager().await;
            let enabled = if let Some(ref mgr) = global_manager {
                mgr.is_enabled().await
            } else {
                false
            };
            let _ = tx_global.send((global_manager, enabled));
        });
        
        // Get global browser state
        let (global_manager, global_enabled) = rx_global.recv_timeout(std::time::Duration::from_millis(100))
            .unwrap_or((None, false));
        
        // Use global manager if it exists and is enabled, otherwise use local
        let (browser_manager, browser_enabled) = if global_enabled && global_manager.is_some() {
            (global_manager.unwrap(), true)
        } else if local_enabled {
            (local_browser_manager, true)
        } else if let Some(global) = global_manager {
            // Global exists but not enabled, still prefer it over local
            (global, false)
        } else {
            (local_browser_manager, false)
        };
        
        // Get current browser URL for display
        let mut browser_url = None;
        
        if browser_enabled {
            tracing::info!("Browser is enabled, attempting to capture screenshot...");
            
            // We need to capture screenshots synchronously before sending the message
            // Use a channel to get the result from the async task
            let (tx, rx) = std::sync::mpsc::channel();
            let browser_manager_capture = browser_manager.clone();
            
            tokio::spawn(async move {
                tracing::info!("Starting async screenshot capture...");
                let result = browser_manager_capture.capture_screenshot_with_url().await;
                match &result {
                    Ok((paths, _)) => tracing::info!("Screenshot capture succeeded with {} images", paths.len()),
                    Err(e) => tracing::error!("Screenshot capture failed: {}", e),
                }
                let _ = tx.send(result);
            });
            
            // Wait for screenshot capture (with timeout)
            match rx.recv_timeout(std::time::Duration::from_secs(5)) {
                Ok(Ok((screenshot_paths, url))) => {
                    tracing::info!("Captured {} browser screenshot(s)", screenshot_paths.len());
                    browser_screenshot_count = screenshot_paths.len();
                    let screenshot_url = url.clone();
                    browser_url = url.clone();
                    
                    // Save the first screenshot path and URL for display in the TUI
                    if let Some(first_path) = screenshot_paths.first() {
                        if let Ok(mut latest) = self.latest_browser_screenshot.lock() {
                            let url_string = url.unwrap_or_else(|| "Browser".to_string());
                            *latest = Some((first_path.clone(), url_string));
                        }
                    }
                    
                    // Add browser screenshots directly to items (ephemeral - not stored in image_paths for history)
                    for path in screenshot_paths {
                        tracing::info!("Browser screenshot attached: {}", path.display());
                        // Check if the file actually exists
                        if path.exists() {
                            tracing::info!("Screenshot file exists at: {}", path.display());
                            // Add as EphemeralImage with metadata for identification
                            let timestamp = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            let metadata = format!("screenshot:{}:{}", timestamp, screenshot_url.as_deref().unwrap_or("unknown"));
                            items.push(InputItem::EphemeralImage { 
                                path,
                                metadata: Some(metadata),
                            });
                        } else {
                            tracing::error!("Screenshot file does not exist at: {}", path.display());
                        }
                    }
                }
                Ok(Err(e)) => {
                    tracing::error!("Failed to capture browser screenshot: {}", e);
                }
                Err(_) => {
                    tracing::error!("Browser screenshot capture timed out");
                }
            }
        } else {
            tracing::info!("Browser is not enabled, skipping screenshot capture");
        }

        if !actual_text.is_empty() {
            items.push(InputItem::Text { text: actual_text.clone() });
        }

        // Add user-provided images (these are persistent in history)
        for path in image_paths {
            items.push(InputItem::LocalImage { path });
        }

        if items.is_empty() {
            return;
        }

        // Debug logging for what we're sending
        let ephemeral_count = items.iter().filter(|item| {
            matches!(item, InputItem::EphemeralImage { .. })
        }).count();
        let total_items = items.len();
        if ephemeral_count > 0 {
            tracing::info!("Sending {} items to model (including {} ephemeral images)", total_items, ephemeral_count);
        }

        self.codex_op_tx
            .send(Op::UserInput { items })
            .unwrap_or_else(|e| {
                tracing::error!("failed to send message: {e}");
            });

        // Persist the original text to cross-session message history.
        if !original_text.is_empty() {
            self.codex_op_tx
                .send(Op::AddToHistory { text: original_text.clone() })
                .unwrap_or_else(|e| {
                    tracing::error!("failed to send AddHistory op: {e}");
                });
        }

        // Show text in conversation history, with a note about browser screenshots
        if !original_text.is_empty() {
            let display_text = if browser_screenshot_count > 0 {
                if let Some(url) = browser_url {
                    format!("{}\n\n[Browser: {}]", original_text, url)
                } else {
                    format!("{}\n\n[Browser screenshot attached]", original_text)
                }
            } else {
                original_text.clone()
            };
            self.add_to_history(HistoryCell::new_user_prompt(display_text));
        } else if browser_screenshot_count > 0 {
            let prompt_text = if let Some(url) = browser_url {
                format!("[Browser: {}]", url)
            } else {
                "[Browser screenshot attached]".to_string()
            };
            self.add_to_history(HistoryCell::new_user_prompt(prompt_text));
        }
    }

    #[allow(dead_code)]
    pub(crate) fn set_mouse_status_message(&mut self, message: &str) {
        self.bottom_pane.update_status_text(message.to_string());
    }
    
    pub(crate) fn handle_mouse_event(&mut self, mouse_event: crossterm::event::MouseEvent) {
        use crossterm::event::{MouseEventKind, KeyModifiers};
        
        // Check if Shift is held - if so, let the terminal handle selection
        if mouse_event.modifiers.contains(KeyModifiers::SHIFT) {
            // Don't handle any mouse events when Shift is held
            // This allows the terminal's native text selection to work
            return;
        }
        
        match mouse_event.kind {
            MouseEventKind::ScrollUp => {
                // Scroll up in chat history (increase offset from bottom)
                self.scroll_offset = self.scroll_offset.saturating_add(3);
                self.app_event_tx.send(AppEvent::RequestRedraw);
            }
            MouseEventKind::ScrollDown => {
                // Scroll down in chat history (decrease offset, towards bottom)
                if self.scroll_offset >= 3 {
                    self.scroll_offset = self.scroll_offset.saturating_sub(3);
                    self.app_event_tx.send(AppEvent::RequestRedraw);
                } else {
                    self.scroll_offset = 0;
                }
            }
            _ => {
                // Ignore other mouse events for now
            }
        }
    }

    pub(crate) fn handle_codex_event(&mut self, event: Event) {
        let Event { id, msg } = event;
        match msg {
            EventMsg::SessionConfigured(event) => {
                self.bottom_pane
                    .set_history_metadata(event.history_log_id, event.history_entry_count);
                // Record session information at the top of the conversation.
                // Only show welcome message on first SessionConfigured event
                let is_first = !self.welcome_shown;
                if is_first {
                    self.welcome_shown = true;
                }
                self.add_to_history(HistoryCell::new_session_info(&self.config, event, is_first));

                if let Some(user_message) = self.initial_user_message.take() {
                    // If the user provided an initial message, add it to the
                    // conversation history.
                    self.submit_user_message(user_message);
                }

                self.request_redraw();
            }
            EventMsg::AgentMessage(AgentMessageEvent { message }) => {
                // AgentMessage: if no deltas were streamed, render the final text.
                if self.current_stream != Some(StreamKind::Answer) && !message.is_empty() {
                    self.begin_stream(StreamKind::Answer);
                    self.stream_push_and_maybe_commit(&message);
                }
                self.finalize_stream(StreamKind::Answer);
                self.request_redraw();
            }
            EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta }) => {
                self.begin_stream(StreamKind::Answer);
                self.answer_buffer.push_str(&delta);
                self.stream_push_and_maybe_commit(&delta);
                self.request_redraw();
            }
            EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent { delta }) => {
                // Stream CoT into the live pane; keep input visible and commit
                // overflow rows incrementally to scrollback.
                self.begin_stream(StreamKind::Reasoning);
                self.reasoning_buffer.push_str(&delta);
                self.stream_push_and_maybe_commit(&delta);
                self.request_redraw();
            }
            EventMsg::AgentReasoning(AgentReasoningEvent { text }) => {
                // Final reasoning: if no deltas were streamed, render the final text.
                if self.current_stream != Some(StreamKind::Reasoning) && !text.is_empty() {
                    self.begin_stream(StreamKind::Reasoning);
                    self.stream_push_and_maybe_commit(&text);
                }
                self.finalize_stream(StreamKind::Reasoning);
                self.request_redraw();
            }
            EventMsg::AgentReasoningRawContentDelta(AgentReasoningRawContentDeltaEvent {
                delta,
            }) => {
                // Treat raw reasoning content the same as summarized reasoning for UI flow.
                self.begin_stream(StreamKind::Reasoning);
                self.reasoning_buffer.push_str(&delta);
                self.stream_push_and_maybe_commit(&delta);
                self.request_redraw();
            }
            EventMsg::AgentReasoningRawContent(AgentReasoningRawContentEvent { text }) => {
                // Final raw reasoning: if no deltas were streamed, render the final text.
                if self.current_stream != Some(StreamKind::Reasoning) && !text.is_empty() {
                    self.begin_stream(StreamKind::Reasoning);
                    self.stream_push_and_maybe_commit(&text);
                }
                self.finalize_stream(StreamKind::Reasoning);
                self.request_redraw();
            }
            EventMsg::TaskStarted => {
                self.bottom_pane.clear_ctrl_c_quit_hint();
                self.bottom_pane.set_task_running(true);
                // Replace composer with single-line spinner while waiting.
                self.bottom_pane
                    .update_status_text("waiting for model".to_string());
                self.request_redraw();
            }
            EventMsg::TaskComplete(TaskCompleteEvent {
                last_agent_message: _,
            }) => {
                self.bottom_pane.set_task_running(false);
                self.bottom_pane.clear_live_ring();
                self.request_redraw();
            }
            EventMsg::TokenCount(token_usage) => {
                self.total_token_usage = add_token_usage(&self.total_token_usage, &token_usage);
                self.last_token_usage = token_usage;
                self.bottom_pane.set_token_usage(
                    self.total_token_usage.clone(),
                    self.last_token_usage.clone(),
                    self.config.model_context_window,
                );
            }
            EventMsg::Error(ErrorEvent { message }) => {
                self.add_to_history(HistoryCell::new_error_event(message.clone()));
                self.bottom_pane.set_task_running(false);
                self.bottom_pane.clear_live_ring();
                self.live_builder = RowBuilder::new(self.live_builder.width());
                self.current_stream = None;
                self.stream_header_emitted = false;
                self.answer_buffer.clear();
                self.reasoning_buffer.clear();
                self.content_buffer.clear();
                self.request_redraw();
            }
            EventMsg::PlanUpdate(update) => {
                // Commit plan updates directly to history (no status-line preview).
                self.add_to_history(HistoryCell::new_plan_update(update));
            }
            EventMsg::ExecApprovalRequest(ExecApprovalRequestEvent {
                call_id: _,
                command,
                cwd,
                reason,
            }) => {
                self.finalize_active_stream();
                let request = ApprovalRequest::Exec {
                    id,
                    command,
                    cwd,
                    reason,
                };
                self.bottom_pane.push_approval_request(request);
                self.request_redraw();
            }
            EventMsg::ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent {
                call_id: _,
                changes,
                reason,
                grant_root,
            }) => {
                self.finalize_active_stream();
                // ------------------------------------------------------------------
                // Before we even prompt the user for approval we surface the patch
                // summary in the main conversation so that the dialog appears in a
                // sensible chronological order:
                //   (1) codex → proposes patch (HistoryCell::PendingPatch)
                //   (2) UI → asks for approval (BottomPane)
                // This mirrors how command execution is shown (command begins →
                // approval dialog) and avoids surprising the user with a modal
                // prompt before they have seen *what* is being requested.
                // ------------------------------------------------------------------
                self.add_to_history(HistoryCell::new_patch_event(
                    PatchEventType::ApprovalRequest,
                    changes,
                ));

                // Now surface the approval request in the BottomPane as before.
                let request = ApprovalRequest::ApplyPatch {
                    id,
                    reason,
                    grant_root,
                };
                self.bottom_pane.push_approval_request(request);
                self.request_redraw();
            }
            EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
                call_id,
                command,
                cwd: _,
                parsed_cmd,
            }) => {
                self.finalize_active_stream();
                // Ensure the status indicator is visible while the command runs.
                self.bottom_pane
                    .update_status_text("running command".to_string());
                self.running_commands.insert(
                    call_id,
                    RunningCommand {
                        command: command.clone(),
                        parsed: parsed_cmd.clone(),
                    },
                );
                self.active_history_cell = Some(HistoryCell::new_active_exec_command(command, parsed_cmd));
            }
            EventMsg::ExecCommandOutputDelta(_) => {
                // TODO
            }
            EventMsg::PatchApplyBegin(PatchApplyBeginEvent {
                call_id: _,
                auto_approved,
                changes,
            }) => {
                self.add_to_history(HistoryCell::new_patch_event(
                    PatchEventType::ApplyBegin { auto_approved },
                    changes,
                ));
            }
            EventMsg::PatchApplyEnd(event) => {
                if !event.success {
                    self.add_to_history(HistoryCell::new_patch_apply_failure(event.stderr));
                }
            }
            EventMsg::ExecCommandEnd(ExecCommandEndEvent {
                call_id,
                exit_code,
                duration: _,
                stdout,
                stderr,
            }) => {
                // Compute summary before moving stdout into the history cell.
                let cmd = self.running_commands.remove(&call_id);
                self.active_history_cell = None;
                let (command, parsed) = cmd
                    .map(|cmd| (cmd.command, cmd.parsed))
                    .unwrap_or_else(|| (vec![call_id], vec![]));
                self.add_to_history(HistoryCell::new_completed_exec_command(
                    command,
                    parsed,
                    CommandOutput {
                        exit_code,
                        stdout,
                        stderr,
                    },
                ));
            }
            EventMsg::McpToolCallBegin(McpToolCallBeginEvent {
                call_id: _,
                invocation,
            }) => {
                self.finalize_active_stream();
                self.add_to_history(HistoryCell::new_active_mcp_tool_call(invocation));
            }
            EventMsg::McpToolCallEnd(McpToolCallEndEvent {
                call_id: _,
                duration,
                invocation,
                result,
            }) => {
                self.add_to_history(HistoryCell::new_completed_mcp_tool_call(
                    80,
                    invocation,
                    duration,
                    result
                        .as_ref()
                        .map(|r| r.is_error.unwrap_or(false))
                        .unwrap_or(false),
                    result,
                ));
            }
            EventMsg::GetHistoryEntryResponse(event) => {
                let codex_core::protocol::GetHistoryEntryResponseEvent {
                    offset,
                    log_id,
                    entry,
                } = event;

                // Inform bottom pane / composer.
                self.bottom_pane
                    .on_history_entry_response(log_id, offset, entry.map(|e| e.text));
            }
            EventMsg::ShutdownComplete => {
                self.app_event_tx.send(AppEvent::ExitRequest);
            }
            EventMsg::TurnDiff(TurnDiffEvent { unified_diff }) => {
                info!("TurnDiffEvent: {unified_diff}");
            }
            EventMsg::BackgroundEvent(BackgroundEventEvent { message }) => {
                info!("BackgroundEvent: {message}");
            }
            EventMsg::BrowserScreenshotUpdate(BrowserScreenshotUpdateEvent { screenshot_path, url }) => {
                tracing::info!("Received browser screenshot update: {} at URL: {}", screenshot_path.display(), url);
                
                // Update the latest screenshot and URL for display
                if let Ok(mut latest) = self.latest_browser_screenshot.lock() {
                    let old_url = latest.as_ref().map(|(_, u)| u.clone());
                    *latest = Some((screenshot_path.clone(), url.clone()));
                    if old_url.as_ref() != Some(&url) {
                        tracing::info!("Browser URL changed from {:?} to {}", old_url, url);
                    }
                    tracing::debug!("Updated browser screenshot display with path: {} and URL: {}", screenshot_path.display(), url);
                } else {
                    tracing::warn!("Failed to acquire lock for browser screenshot update");
                }
                
                // Request a redraw to update the display immediately
                self.app_event_tx.send(AppEvent::RequestRedraw);
            }
        }
    }

    /// Update the live log preview while a task is running.
    pub(crate) fn update_latest_log(&mut self, line: String) {
        if self.bottom_pane.is_task_running() {
            self.bottom_pane.update_status_text(line);
        }
    }

    fn request_redraw(&mut self) {
        self.app_event_tx.send(AppEvent::RequestRedraw);
    }

    pub(crate) fn add_diff_output(&mut self, diff_output: String) {
        self.add_to_history(HistoryCell::new_diff_output(diff_output.clone()));
    }

    pub(crate) fn add_status_output(&mut self) {
        self.add_to_history(HistoryCell::new_status_output(
            &self.config,
            &self.total_token_usage,
        ));
    }

    pub(crate) fn add_prompts_output(&mut self) {
        self.add_to_history(HistoryCell::new_prompts_output());
    }

    pub(crate) fn handle_reasoning_command(&mut self, command_text: String) {
        // Parse the command to extract the parameter
        let parts: Vec<&str> = command_text.trim().split_whitespace().collect();
        
        if parts.len() > 1 {
            // User specified a level: /reasoning high
            let new_effort = match parts[1].to_lowercase().as_str() {
                "low" => ReasoningEffort::Low,
                "medium" | "med" => ReasoningEffort::Medium,
                "high" => ReasoningEffort::High,
                "none" | "off" => ReasoningEffort::None,
                _ => {
                    // Invalid parameter, show error and return
                    let message = format!(
                        "Invalid reasoning level: '{}'. Use: low, medium, high, or none",
                        parts[1]
                    );
                    self.add_to_history(HistoryCell::new_error_event(message));
                    return;
                }
            };
            self.set_reasoning_effort(new_effort);
        } else {
            // No parameter - show interactive selection UI
            self.bottom_pane.show_reasoning_selection(self.config.model_reasoning_effort);
            return;
        }
    }
    
    pub(crate) fn set_reasoning_effort(&mut self, new_effort: ReasoningEffort) {
        
        // Update the config
        self.config.model_reasoning_effort = new_effort;
        
        // Send ConfigureSession op to update the backend
        let op = Op::ConfigureSession {
            provider: self.config.model_provider.clone(),
            model: self.config.model.clone(),
            model_reasoning_effort: new_effort,
            model_reasoning_summary: self.config.model_reasoning_summary,
            user_instructions: self.config.user_instructions.clone(),
            base_instructions: self.config.base_instructions.clone(),
            approval_policy: self.config.approval_policy.clone(),
            sandbox_policy: self.config.sandbox_policy.clone(),
            disable_response_storage: self.config.disable_response_storage,
            notify: self.config.notify.clone(),
            cwd: self.config.cwd.clone(),
            resume_path: None,
        };
        
        self.submit_op(op);
        
        // Add status message to history
        self.add_to_history(HistoryCell::new_reasoning_output(new_effort));
    }

    /// Forward file-search results to the bottom pane.
    pub(crate) fn apply_file_search_result(&mut self, query: String, matches: Vec<FileMatch>) {
        self.bottom_pane.on_file_search_result(query, matches);
    }
    
    pub(crate) fn show_theme_selection(&mut self) {
        self.bottom_pane.show_theme_selection(self.config.tui.theme.name);
    }
    
    pub(crate) fn set_theme(&mut self, new_theme: codex_core::config_types::ThemeName) {
        // Update the config
        self.config.tui.theme.name = new_theme;
        
        // Save the theme to config file
        self.save_theme_to_config(new_theme);
        
        // Add confirmation message to history
        let theme_name = match new_theme {
            // Light themes
            codex_core::config_types::ThemeName::LightPhoton => "Light - Photon",
            codex_core::config_types::ThemeName::LightPrismRainbow => "Light - Prism Rainbow",
            codex_core::config_types::ThemeName::LightVividTriad => "Light - Vivid Triad",
            codex_core::config_types::ThemeName::LightPorcelain => "Light - Porcelain",
            codex_core::config_types::ThemeName::LightSandbar => "Light - Sandbar",
            codex_core::config_types::ThemeName::LightGlacier => "Light - Glacier",
            // Dark themes
            codex_core::config_types::ThemeName::DarkCarbonNight => "Dark - Carbon Night",
            codex_core::config_types::ThemeName::DarkShinobiDusk => "Dark - Shinobi Dusk",
            codex_core::config_types::ThemeName::DarkOledBlackPro => "Dark - OLED Black Pro",
            codex_core::config_types::ThemeName::DarkAmberTerminal => "Dark - Amber Terminal",
            codex_core::config_types::ThemeName::DarkAuroraFlux => "Dark - Aurora Flux",
            codex_core::config_types::ThemeName::DarkCharcoalRainbow => "Dark - Charcoal Rainbow",
            codex_core::config_types::ThemeName::DarkZenGarden => "Dark - Zen Garden",
            codex_core::config_types::ThemeName::DarkPaperLightPro => "Dark - Paper Light Pro",
            codex_core::config_types::ThemeName::Custom => "Custom",
        };
        let message = format!("✓ Theme changed to {}", theme_name);
        self.add_to_history(HistoryCell::new_background_event(message));
    }
    
    fn save_theme_to_config(&self, new_theme: codex_core::config_types::ThemeName) {
        // For now, just log the theme change - config saving could be implemented
        // using the core config system in a future update
        let theme_str = match new_theme {
            // Light themes
            codex_core::config_types::ThemeName::LightPhoton => "light-photon",
            codex_core::config_types::ThemeName::LightPrismRainbow => "light-prism-rainbow",
            codex_core::config_types::ThemeName::LightVividTriad => "light-vivid-triad",
            codex_core::config_types::ThemeName::LightPorcelain => "light-porcelain",
            codex_core::config_types::ThemeName::LightSandbar => "light-sandbar",
            codex_core::config_types::ThemeName::LightGlacier => "light-glacier",
            // Dark themes
            codex_core::config_types::ThemeName::DarkCarbonNight => "dark-carbon-night",
            codex_core::config_types::ThemeName::DarkShinobiDusk => "dark-shinobi-dusk",
            codex_core::config_types::ThemeName::DarkOledBlackPro => "dark-oled-black-pro",
            codex_core::config_types::ThemeName::DarkAmberTerminal => "dark-amber-terminal",
            codex_core::config_types::ThemeName::DarkAuroraFlux => "dark-aurora-flux",
            codex_core::config_types::ThemeName::DarkCharcoalRainbow => "dark-charcoal-rainbow",
            codex_core::config_types::ThemeName::DarkZenGarden => "dark-zen-garden",
            codex_core::config_types::ThemeName::DarkPaperLightPro => "dark-paper-light-pro",
            codex_core::config_types::ThemeName::Custom => "custom",
        };
        tracing::info!("Theme changed to: {}", theme_str);
        // Note: To persist the theme, add the following to your config.toml:
        // [tui.theme]
        // name = "{}"
    }

    pub(crate) fn on_esc(&mut self) -> bool {
        if self.bottom_pane.is_task_running() {
            self.interrupt_running_task();
            return true;
        }
        false
    }

    /// Handle Ctrl-C key press.
    /// Returns CancellationEvent::Handled if the event was consumed by the UI, or
    /// CancellationEvent::Ignored if the caller should handle it (e.g. exit).
    pub(crate) fn on_ctrl_c(&mut self) -> CancellationEvent {
        match self.bottom_pane.on_ctrl_c() {
            CancellationEvent::Handled => return CancellationEvent::Handled,
            CancellationEvent::Ignored => {}
        }
        if self.bottom_pane.is_task_running() {
            self.interrupt_running_task();
            CancellationEvent::Ignored
        } else if self.bottom_pane.ctrl_c_quit_hint_visible() {
            self.submit_op(Op::Shutdown);
            CancellationEvent::Handled
        } else {
            self.bottom_pane.show_ctrl_c_quit_hint();
            CancellationEvent::Ignored
        }
    }

    pub(crate) fn on_ctrl_z(&mut self) {
        self.interrupt_running_task();
    }

    pub(crate) fn composer_is_empty(&self) -> bool {
        self.bottom_pane.composer_is_empty()
    }

    /// Forward an `Op` directly to codex.
    pub(crate) fn submit_op(&self, op: Op) {
        if let Err(e) = self.codex_op_tx.send(op) {
            tracing::error!("failed to submit op: {e}");
        }
    }

    pub(crate) fn handle_browser_command(&mut self, command_text: String) {
        // Parse the browser subcommand
        let parts: Vec<&str> = command_text.trim().split_whitespace().collect();
        
        let browser_manager = self.browser_manager.clone();
        let response = if parts.len() > 1 {
            let first_arg = parts[1];
            
            // Check if the first argument looks like a URL (has a dot or protocol)
            let is_url = first_arg.contains("://") || first_arg.contains(".");
            
            if is_url {
                // It's a URL - enable browser mode and navigate to it
                let url = parts[1..].join(" ");
                
                // Ensure URL has protocol
                let full_url = if !url.contains("://") {
                    format!("https://{}", url)
                } else {
                    url.clone()
                };
                
                // Enable browser mode
                browser_manager.set_enabled_sync(true);
                
                // Navigate to URL and wait for it to load
                let browser_manager_open = browser_manager.clone();
                let url_for_goto = full_url.clone();
                tokio::spawn(async move {
                    match browser_manager_open.goto(&url_for_goto).await {
                        Ok(result) => {
                            tracing::info!("Browser opened to: {} (title: {:?})", result.url, result.title);
                            // Add a small delay to ensure page is fully rendered
                            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                            tracing::info!("Page should be loaded now");
                        }
                        Err(e) => {
                            tracing::error!("Failed to open browser: {}", e);
                        }
                    }
                });
                
                format!("Browser mode enabled. Opening: {}\nScreenshots will be attached to model requests.", full_url)
            } else {
                // It's a subcommand
                match first_arg {
                    "off" => {
                        // Disable browser mode
                        browser_manager.set_enabled_sync(false);
                        // Clear the screenshot popup
                        if let Ok(mut screenshot_lock) = self.latest_browser_screenshot.lock() {
                            *screenshot_lock = None;
                        }
                        // Close any open browser
                        let browser_manager_close = browser_manager.clone();
                        tokio::spawn(async move {
                            if let Err(e) = browser_manager_close.close().await {
                                tracing::error!("Failed to close browser: {}", e);
                            }
                        });
                        self.app_event_tx.send(AppEvent::RequestRedraw);
                        "Browser mode disabled.".to_string()
                    }
                    "status" => {
                        // Get status from BrowserManager
                        browser_manager.get_status_sync()
                    }
                "fullpage" => {
                    if parts.len() > 2 {
                        match parts[2] {
                            "on" => {
                                // Enable full-page mode
                                browser_manager.set_fullpage_sync(true);
                                "Full-page screenshot mode enabled (max 8 segments).".to_string()
                            }
                            "off" => {
                                // Disable full-page mode
                                browser_manager.set_fullpage_sync(false);
                                "Full-page screenshot mode disabled.".to_string()
                            }
                            _ => "Usage: /browser fullpage [on|off]".to_string()
                        }
                    } else {
                        "Usage: /browser fullpage [on|off]".to_string()
                    }
                }
                "config" => {
                    if parts.len() > 3 {
                        let key = parts[2];
                        let value = parts[3..].join(" ");
                        // Update browser config
                        match key {
                            "viewport" => {
                                // Parse viewport dimensions like "1920x1080"
                                if let Some((width_str, height_str)) = value.split_once('x') {
                                    if let (Ok(width), Ok(height)) = (width_str.parse::<u32>(), height_str.parse::<u32>()) {
                                        browser_manager.set_viewport_sync(width, height);
                                        format!("Browser viewport updated: {}x{}", width, height)
                                    } else {
                                        "Invalid viewport format. Use: /browser config viewport 1920x1080".to_string()
                                    }
                                } else {
                                    "Invalid viewport format. Use: /browser config viewport 1920x1080".to_string()
                                }
                            }
                            "segments_max" => {
                                if let Ok(max) = value.parse::<usize>() {
                                    browser_manager.set_segments_max_sync(max);
                                    format!("Browser segments_max updated: {}", max)
                                } else {
                                    "Invalid segments_max value. Use a number.".to_string()
                                }
                            }
                            _ => format!("Unknown config key: {}. Available: viewport, segments_max", key)
                        }
                    } else {
                        "Usage: /browser config <key> <value>\nAvailable keys: viewport, segments_max".to_string()
                    }
                }
                    _ => {
                        format!("Unknown browser command: '{}'\nUsage: /browser <url> | off | status | fullpage | config", first_arg)
                    }
                }
            }
        } else {
            "Browser commands:\n• /browser <url> - Open URL and enable browser mode\n• /browser off - Disable browser mode\n• /browser status - Show current status\n• /browser fullpage [on|off] - Toggle full-page mode\n• /browser config <key> <value> - Update configuration".to_string()
        };
        
        // Add the response to the UI as a background event
        let lines = response.lines().map(|line| Line::from(line.to_string())).collect();
        self.add_to_history(HistoryCell::BackgroundEvent {
            view: TextBlock::new(lines),
        });
    }

    /// Programmatically submit a user text message as if typed in the
    /// composer. The text will be added to conversation history and sent to
    /// the agent. This also handles slash command expansion.
    pub(crate) fn submit_text_message(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        self.submit_user_message(text.into());
    }

    pub(crate) fn token_usage(&self) -> &TokenUsage {
        &self.total_token_usage
    }

    pub(crate) fn clear_token_usage(&mut self) {
        self.total_token_usage = TokenUsage::default();
        self.bottom_pane.set_token_usage(
            self.total_token_usage.clone(),
            self.last_token_usage.clone(),
            self.config.model_context_window,
        );
    }

    pub fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        let layout_areas = self.layout_areas(area);
        let bottom_pane_area = if layout_areas.len() == 4 {
            layout_areas[3]
        } else {
            layout_areas[2]
        };
        self.bottom_pane.cursor_pos(bottom_pane_area)
    }

    fn measured_font_size(&self) -> (u16, u16) {
        let default_guess = if std::env::var("TERM_PROGRAM").unwrap_or_default() == "iTerm.app" {
            (7, 15)
        } else {
            (8, 16)
        };
        *self.cached_cell_size.get_or_init(|| {
            let size = crate::terminal_info::get_cell_size_pixels().unwrap_or(default_guess);
            
            // HACK: On macOS Retina displays, terminals often report physical pixels
            // but ratatui-image expects logical pixels. If we detect suspiciously 
            // large cell sizes (likely 2x scaled), divide by 2.
            #[cfg(target_os = "macos")]
            {
                if size.0 >= 14 && size.1 >= 28 {
                    // Likely Retina display reporting physical pixels
                    tracing::info!("Detected likely Retina display, adjusting cell size from {:?} to {:?}", 
                        size, (size.0 / 2, size.1 / 2));
                    return (size.0 / 2, size.1 / 2);
                }
            }
            
            size
        })
    }
    
    fn get_git_branch(&self) -> Option<String> {
        use std::process::Command;
        
        let output = Command::new("git")
            .arg("rev-parse")
            .arg("--abbrev-ref")
            .arg("HEAD")
            .current_dir(&self.config.cwd)
            .output()
            .ok()?;
        
        if output.status.success() {
            let branch = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_string();
            if !branch.is_empty() && branch != "HEAD" {
                Some(branch)
            } else {
                // Try to get short commit hash if in detached HEAD state
                let commit_output = Command::new("git")
                    .arg("rev-parse")
                    .arg("--short")
                    .arg("HEAD")
                    .current_dir(&self.config.cwd)
                    .output()
                    .ok()?;
                
                if commit_output.status.success() {
                    let commit = String::from_utf8_lossy(&commit_output.stdout)
                        .trim()
                        .to_string();
                    if !commit.is_empty() {
                        Some(format!("detached: {}", commit))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        } else {
            None
        }
    }
    
    fn render_status_bar(&self, area: Rect, buf: &mut Buffer) {
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Paragraph, Block, Borders};
        use ratatui::style::{Style, Modifier};
        use ratatui::layout::Margin;
        use crate::exec_command::relativize_to_home;
        
        // Add same horizontal padding as the Message input (2 chars on each side)
        let horizontal_padding = 2u16;
        let padded_area = Rect {
            x: area.x + horizontal_padding,
            y: area.y,
            width: area.width.saturating_sub(horizontal_padding * 2),
            height: area.height,
        };
        
        // Get current working directory string
        let cwd_str = match relativize_to_home(&self.config.cwd) {
            Some(rel) if !rel.as_os_str().is_empty() => format!("~/{}", rel.display()),
            Some(_) => "~".to_string(),
            None => self.config.cwd.display().to_string(),
        };
        
        // Build status line spans
        let mut status_spans = vec![
            Span::styled("Coder", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" • "),
            Span::styled("Model: ", Style::default().dim()),
            Span::styled(
                format!("{}", self.config.model),
                Style::default().fg(crate::colors::info())
            ),
            Span::raw(" • "),
            Span::styled("Directory: ", Style::default().dim()),
            Span::styled(cwd_str, Style::default().fg(crate::colors::info())),
        ];
        
        // Add git branch if available
        if let Some(branch) = self.get_git_branch() {
            status_spans.push(Span::raw(" • "));
            status_spans.push(Span::styled("Branch: ", Style::default().dim()));
            status_spans.push(Span::styled(
                branch,
                Style::default().fg(crate::colors::success_green())
            ));
        }
        
        let status_line = Line::from(status_spans);
        
        // Create box border similar to Message input
        let status_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()));
        
        // Add padding inside the box (1 char horizontal only, like Message input)
        let inner_area = status_block.inner(padded_area);
        let padded_inner = inner_area.inner(Margin::new(1, 0));
        
        // Render the block first
        status_block.render(padded_area, buf);
        
        // Then render the text inside with padding
        let status_widget = Paragraph::new(vec![status_line]);
        ratatui::widgets::Widget::render(status_widget, padded_inner, buf);
    }

fn render_screenshot_highlevel(&self, path: &PathBuf, area: Rect, buf: &mut Buffer) {
    use image::GenericImageView;                         // for dimensions()
    use ratatui_image::{Resize, Image};
    use ratatui_image::picker::{Picker, ProtocolType};
    use ratatui::widgets::Widget;

    // open + decode
    let reader = match image::ImageReader::open(path) {
        Ok(r) => r,
        Err(_) => { self.render_screenshot_placeholder(path, area, buf); return; }
    };
    let dyn_img = match reader.decode() {
        Ok(img) => img,
        Err(_) => { self.render_screenshot_placeholder(path, area, buf); return; }
    };
    let (img_w, img_h) = dyn_img.dimensions();

    // picker (Retina 2x workaround preserved)
    let mut cached_picker = self.cached_picker.borrow_mut();
    if cached_picker.is_none() {
        let (fw, fh) = self.measured_font_size();
        *cached_picker = Some(Picker::from_fontsize((fw * 2, fh * 2)));
    }
    let picker = cached_picker.as_ref().unwrap();

    // quantize step by protocol to avoid rounding bias
    let (qx, qy): (u16, u16) = match picker.protocol_type() {
        ProtocolType::Halfblocks => (1, 2), // half-block cell = 1 col x 2 half-rows
        _ => (1, 1),                        // pixel protocols (Kitty/iTerm2/Sixel)
    };

    // terminal cell aspect
    let (cw, ch) = self.measured_font_size();
    let cols = area.width as u32;
    let rows = area.height as u32;
    let cw = cw as u32;
    let ch = ch as u32;

    // fit (floor), then choose limiting dimension
    let mut rows_by_w = (cols * cw * img_h) / (img_w * ch);
    if rows_by_w == 0 { rows_by_w = 1; }
    let mut cols_by_h = (rows * ch * img_w) / (img_h * cw);
    if cols_by_h == 0 { cols_by_h = 1; }

    let (used_cols, used_rows) = if rows_by_w <= rows {
        (cols, rows_by_w)
    } else {
        (cols_by_h, rows)
    };

    // quantize to protocol grid
    let mut used_cols_u16 = used_cols as u16;
    let mut used_rows_u16 = used_rows as u16;
    if qx > 1 {
        let rem = used_cols_u16 % qx;
        if rem != 0 { used_cols_u16 = used_cols_u16.saturating_sub(rem).max(qx); }
    }
    if qy > 1 {
        let rem = used_rows_u16 % qy;
        if rem != 0 { used_rows_u16 = used_rows_u16.saturating_sub(rem).max(qy); }
    }
    used_cols_u16 = used_cols_u16.min(area.width).max(1);
    used_rows_u16 = used_rows_u16.min(area.height).max(1);

    // center both axes
    let hpad = (area.width.saturating_sub(used_cols_u16)) / 2;
    let vpad = (area.height.saturating_sub(used_rows_u16)) / 2;
    let target = Rect {
        x: area.x + hpad,
        y: area.y + vpad,
        width: used_cols_u16,
        height: used_rows_u16,
    };

    // cache by (path, target)
    let needs_recreate = {
        let cached = self.cached_image_protocol.borrow();
        match cached.as_ref() {
            Some((cached_path, cached_rect, _)) => cached_path != path || *cached_rect != target,
            None => true,
        }
    };
    if needs_recreate {
        match picker.new_protocol(dyn_img, target, Resize::Fit(None)) {
            Ok(protocol) => *self.cached_image_protocol.borrow_mut() = Some((path.clone(), target, protocol)),
            Err(_) => { self.render_screenshot_placeholder(path, area, buf); return; }
        }
    }

    if let Some((_, rect, protocol)) = &*self.cached_image_protocol.borrow() {
        let image = Image::new(protocol);
        Widget::render(image, *rect, buf);
    } else {
        self.render_screenshot_placeholder(path, area, buf);
    }
}

    fn render_screenshot_placeholder(&self, path: &PathBuf, area: Rect, buf: &mut Buffer) {
        use ratatui::widgets::{Paragraph, Block, Borders};
        use ratatui::style::{Style, Modifier};
        
        // Show a placeholder box with screenshot info
        let filename = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("screenshot");
            
        let placeholder_text = format!("[Screenshot]\n{}", filename);
        let placeholder_widget = Paragraph::new(placeholder_text)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(crate::colors::info()))
                .title("Browser"))
            .style(Style::default()
                .fg(crate::colors::text_dim())
                .add_modifier(Modifier::ITALIC))
            .wrap(ratatui::widgets::Wrap { trim: true });
        
        placeholder_widget.render(area, buf);
    }
}

impl ChatWidget<'_> {
    /// Helper function to create a top-right positioned rect with proper aspect ratio
    fn render_browser_hud(&self, area: Rect, buf: &mut Buffer) {
        use ratatui::style::Style;
        use ratatui::widgets::{Block, Borders, Paragraph, Widget};
        use ratatui::text::{Line as RLine, Span};
        
        
        // Add same horizontal padding as the Message input (2 chars on each side)
        let horizontal_padding = 2u16;
        let padded_area = Rect {
            x: area.x + horizontal_padding,
            y: area.y,
            width: area.width.saturating_sub(horizontal_padding * 2),
            height: area.height,
        };

        if let Ok(screenshot_lock) = self.latest_browser_screenshot.lock() {
            if let Some((screenshot_path, url)) = &*screenshot_lock {
                // Split the HUD into two parts: left for preview, right for status (50% width)
                let chunks = Layout::horizontal([
                    Constraint::Percentage(50),
                    Constraint::Percentage(50),
                ])
                .areas::<2>(padded_area);
                
                let screenshot_area = chunks[0];  // Preview on the left
                let info_area = chunks[1];        // Status on the right
                
                // Left side: Screenshot with URL in title
                let screenshot_block = Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {} ", url))  // URL in the title
                    .border_style(Style::default().fg(crate::colors::border()));
                
                let inner_screenshot = screenshot_block.inner(screenshot_area);
                screenshot_block.render(screenshot_area, buf);
                
                // Render the screenshot
                self.render_screenshot_highlevel(screenshot_path, inner_screenshot, buf);
                
                // Right side: Browser status
                let info_block = Block::default()
                    .borders(Borders::ALL)
                    .title(" Browser Status ")
                    .border_style(Style::default().fg(crate::colors::border()));
                
                let inner_info = info_block.inner(info_area);
                info_block.render(info_area, buf);
                
                // Display browser status information
                let mut lines = vec![];
                
                // Add browser status information
                if self.browser_manager.is_enabled_sync() {
                    lines.push(RLine::from(vec![
                        Span::styled("Status: ", Style::default().fg(crate::colors::text_dim())),
                        Span::styled("Active", Style::default().fg(crate::colors::success())),
                    ]));
                } else {
                    lines.push(RLine::from(vec![
                        Span::styled("Status: ", Style::default().fg(crate::colors::text_dim())),
                        Span::styled("Inactive", Style::default().fg(crate::colors::warning())),
                    ]));
                }
                
                let info_paragraph = Paragraph::new(lines);
                info_paragraph.render(inner_info, buf);
            }
        }
    }
    
    fn begin_stream(&mut self, kind: StreamKind) {
        if let Some(current) = self.current_stream {
            if current != kind {
                self.finalize_stream(current);
            }
        }

        if self.current_stream != Some(kind) {
            self.current_stream = Some(kind);
            self.stream_header_emitted = false;
            // Clear any previous live content; we're starting a new stream.
            self.live_builder = RowBuilder::new(self.live_builder.width());
            // Ensure the waiting status is visible (composer replaced).
            self.bottom_pane
                .update_status_text("waiting for model".to_string());
            self.emit_stream_header(kind);
        }
    }

    fn stream_push_and_maybe_commit(&mut self, delta: &str) {
        self.live_builder.push_fragment(delta);

        // Commit overflow rows (small batches) while keeping the last N rows visible.
        let drained = self
            .live_builder
            .drain_commit_ready(self.live_max_rows as usize);
        if !drained.is_empty() {
            let mut lines: Vec<ratatui::text::Line<'static>> = Vec::new();
            if !self.stream_header_emitted {
                match self.current_stream {
                    Some(StreamKind::Reasoning) => {
                        lines.push(ratatui::text::Line::from("thinking".magenta().italic()));
                    }
                    Some(StreamKind::Answer) => {
                        lines.push(ratatui::text::Line::from("codex".magenta().bold()));
                    }
                    None => {}
                }
                self.stream_header_emitted = true;
            }
            for r in drained {
                lines.push(ratatui::text::Line::from(r.text));
            }
            // Add to in-memory history instead of scrollback
            for line in lines {
                self.history_cells.push(HistoryCell::new_text_line(line));
            }
            // Ensure input focus is maintained when adding streamed content
            self.bottom_pane.ensure_input_focus();
            self.app_event_tx.send(AppEvent::RequestRedraw);
        }

        // Update the live ring overlay lines (text-only, newest at bottom).
        let rows = self
            .live_builder
            .display_rows()
            .into_iter()
            .map(|r| ratatui::text::Line::from(r.text))
            .collect::<Vec<_>>();
        self.bottom_pane
            .set_live_ring_rows(self.live_max_rows, rows);
    }

    fn finalize_stream(&mut self, kind: StreamKind) {
        if self.current_stream != Some(kind) {
            // Nothing to do; either already finalized or not the active stream.
            return;
        }
        // Flush any partial line as a full row, then drain all remaining rows.
        self.live_builder.end_line();
        let remaining = self.live_builder.drain_rows();
        // TODO: Re-add markdown rendering for assistant answers and reasoning.
        // When finalizing, pass the accumulated text through `markdown::append_markdown`
        // to build styled `Line<'static>` entries instead of raw plain text lines.
        if !remaining.is_empty() || !self.stream_header_emitted {
            let mut lines: Vec<ratatui::text::Line<'static>> = Vec::new();
            if !self.stream_header_emitted {
                match kind {
                    StreamKind::Reasoning => {
                        lines.push(ratatui::text::Line::from("thinking".magenta().italic()));
                    }
                    StreamKind::Answer => {
                        lines.push(ratatui::text::Line::from("codex".magenta().bold()));
                    }
                }
                self.stream_header_emitted = true;
            }
            for r in remaining {
                lines.push(ratatui::text::Line::from(r.text));
            }
            // Close the block with a blank line for readability.
            lines.push(ratatui::text::Line::from(""));
            // Add to in-memory history instead of scrollback
            for line in lines {
                self.history_cells.push(HistoryCell::new_text_line(line));
            }
            // Ensure input focus is maintained when adding streamed content
            self.bottom_pane.ensure_input_focus();
            self.app_event_tx.send(AppEvent::RequestRedraw);
        }

        // Clear the live overlay and reset state for the next stream.
        self.live_builder = RowBuilder::new(self.live_builder.width());
        self.bottom_pane.clear_live_ring();
        self.current_stream = None;
        self.stream_header_emitted = false;
    }
}

impl WidgetRef for &ChatWidget<'_> {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        use ratatui::style::Style;
        
        // Fill entire area with theme background
        let bg_style = Style::default()
            .bg(crate::colors::background())
            .fg(crate::colors::text());
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].set_style(bg_style);
            }
        }
        
        let layout_areas = self.layout_areas(area);
        let (status_bar_area, hud_area, history_area, bottom_pane_area) = if layout_areas.len() == 4 {
            // Browser HUD is present
            (layout_areas[0], Some(layout_areas[1]), layout_areas[2], layout_areas[3])
        } else {
            // No browser HUD
            (layout_areas[0], None, layout_areas[1], layout_areas[2])
        };
        
        // Render status bar
        self.render_status_bar(status_bar_area, buf);
        
        // Render browser HUD if present
        if let Some(hud_area) = hud_area {
            self.render_browser_hud(hud_area, buf);
        }
        
        // Add horizontal padding to match the bottom pane (3 chars on each side)
        let horizontal_padding = 3u16;
        let padded_history_area = Rect {
            x: history_area.x + horizontal_padding,
            y: history_area.y,
            width: history_area.width.saturating_sub(horizontal_padding * 2),
            height: history_area.height,
        };
        
        // Calculate total height of all history cells with padded width
        let mut total_height = 0u16;
        let mut cell_heights = Vec::new();
        
        for cell in &self.history_cells {
            let h = cell.desired_height(padded_history_area.width);
            cell_heights.push(h);
            total_height += h;
        }
        
        // Add active cell height if present
        if let Some(cell) = &self.active_history_cell {
            let h = cell.desired_height(padded_history_area.width);
            cell_heights.push(h);
            total_height += h;
        }
        
        // Add a small top padding for better visual spacing
        let top_padding = 2u16;
        
        // Start rendering from bottom if total height is less than area height
        let start_y = if total_height < padded_history_area.height.saturating_sub(top_padding) {
            // Align to bottom with top padding
            padded_history_area.y + top_padding.max(padded_history_area.height - total_height)
        } else {
            // Scroll: show most recent messages at bottom
            padded_history_area.y + top_padding
        };
        
        // Render history cells from bottom alignment
        let mut y_offset = if total_height < padded_history_area.height {
            // All content fits, but still apply scroll_offset if user scrolled up
            self.scroll_offset.min(total_height.saturating_sub(1))
        } else {
            // When scrolling, skip older messages that don't fit, plus any user scroll offset
            total_height
                .saturating_sub(padded_history_area.height)
                .saturating_add(self.scroll_offset)
                .min(total_height.saturating_sub(1))
        };
        
        let mut current_y = start_y;
        let mut cell_idx = 0;
        
        // Skip cells that are scrolled off the top
        for (i, _cell) in self.history_cells.iter().enumerate() {
            if y_offset >= cell_heights[i] {
                y_offset = y_offset.saturating_sub(cell_heights[i]);
                cell_idx += 1;
            } else {
                break;
            }
        }
        
        // Check if we have any animation cells that need redrawing
        let has_active_animation = self.history_cells.iter().any(|cell| {
            if let HistoryCell::AnimatedWelcome { start_time, completed } = cell {
                let elapsed = start_time.elapsed();
                let animation_duration = std::time::Duration::from_secs(2);
                elapsed < animation_duration && !completed.get()
            } else {
                false
            }
        });
        
        // Request redraw if we have active animations
        if has_active_animation {
            self.app_event_tx.send(AppEvent::RequestRedraw);
        }
        
        // Render visible history cells
        for (i, cell) in self.history_cells.iter().enumerate().skip(cell_idx) {
            if current_y >= padded_history_area.y + padded_history_area.height {
                break;
            }
            
            let cell_height = cell_heights[i];
            let visible_height = (padded_history_area.y + padded_history_area.height).saturating_sub(current_y);
            
            if visible_height > 0 {
                let cell_area = Rect {
                    x: padded_history_area.x,
                    y: current_y,
                    width: padded_history_area.width,
                    height: visible_height.min(cell_height),
                };
                
                // If we're partially scrolled, adjust the rendering
                if y_offset > 0 && i == cell_idx {
                    // This cell is partially scrolled off
                    // We need to render it with an offset
                    // For now, just render what's visible
                }
                
                cell.render_ref(cell_area, buf);
                current_y += cell_height.saturating_sub(y_offset);
                y_offset = 0; // Only first cell can be partially scrolled
            }
        }
        
        // Render active cell if present and there's room
        if let Some(cell) = &self.active_history_cell {
            if current_y < padded_history_area.y + padded_history_area.height {
                let cell_area = Rect {
                    x: padded_history_area.x,
                    y: current_y,
                    width: padded_history_area.width,
                    height: (padded_history_area.y + padded_history_area.height).saturating_sub(current_y),
                };
                cell.render_ref(cell_area, buf);
            }
        }
        
        // Render the bottom pane directly without a border for now
        // The composer has its own layout with hints at the bottom
        (&self.bottom_pane).render(bottom_pane_area, buf);
    }
}

fn add_token_usage(current_usage: &TokenUsage, new_usage: &TokenUsage) -> TokenUsage {
    let cached_input_tokens = match (
        current_usage.cached_input_tokens,
        new_usage.cached_input_tokens,
    ) {
        (Some(current), Some(new)) => Some(current + new),
        (Some(current), None) => Some(current),
        (None, Some(new)) => Some(new),
        (None, None) => None,
    };
    let reasoning_output_tokens = match (
        current_usage.reasoning_output_tokens,
        new_usage.reasoning_output_tokens,
    ) {
        (Some(current), Some(new)) => Some(current + new),
        (Some(current), None) => Some(current),
        (None, Some(new)) => Some(new),
        (None, None) => None,
    };
    TokenUsage {
        input_tokens: current_usage.input_tokens + new_usage.input_tokens,
        cached_input_tokens,
        output_tokens: current_usage.output_tokens + new_usage.output_tokens,
        reasoning_output_tokens,
        total_tokens: current_usage.total_tokens + new_usage.total_tokens,
    }
}
