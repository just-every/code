use codex_core::config_types::ThemeName;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

use super::bottom_pane_view::BottomPaneView;
use super::BottomPane;

/// Interactive UI for selecting theme
pub(crate) struct ThemeSelectionView {
    original_theme: ThemeName,  // Theme to restore on cancel
    current_theme: ThemeName,   // Currently displayed theme
    selected_index: usize,
    app_event_tx: AppEventSender,
    is_complete: bool,
}

impl ThemeSelectionView {
    pub fn new(current_theme: ThemeName, app_event_tx: AppEventSender) -> Self {
        let themes = Self::get_theme_options();
        let selected_index = themes
            .iter()
            .position(|(t, _, _)| *t == current_theme)
            .unwrap_or(0);
        
        Self {
            original_theme: current_theme,
            current_theme,
            selected_index,
            app_event_tx,
            is_complete: false,
        }
    }

    fn get_theme_options() -> Vec<(ThemeName, &'static str, &'static str)> {
        vec![
            // Light themes (at top)
            (ThemeName::LightPhoton, "Light - Photon", "Clean professional light theme"),
            (ThemeName::LightPrismRainbow, "Light - Prism Rainbow", "Vibrant rainbow accents"),
            (ThemeName::LightVividTriad, "Light - Vivid Triad", "Cyan, pink, amber triad"),
            (ThemeName::LightPorcelain, "Light - Porcelain", "Refined porcelain tones"),
            (ThemeName::LightSandbar, "Light - Sandbar", "Warm sandy beach colors"),
            (ThemeName::LightGlacier, "Light - Glacier", "Cool glacier blues"),
            (ThemeName::DarkPaperLightPro, "Light - Paper Pro", "Premium paper-like"),
            // Dark themes (below)
            (ThemeName::DarkCarbonNight, "Dark - Carbon Night", "Sleek modern dark theme"),
            (ThemeName::DarkShinobiDusk, "Dark - Shinobi Dusk", "Japanese-inspired twilight"),
            (ThemeName::DarkOledBlackPro, "Dark - OLED Black Pro", "True black for OLED displays"),
            (ThemeName::DarkAmberTerminal, "Dark - Amber Terminal", "Retro amber CRT aesthetic"),
            (ThemeName::DarkAuroraFlux, "Dark - Aurora Flux", "Northern lights inspired"),
            (ThemeName::DarkCharcoalRainbow, "Dark - Charcoal Rainbow", "High-contrast accessible"),
            (ThemeName::DarkZenGarden, "Dark - Zen Garden", "Calm and peaceful"),
        ]
    }

    fn move_selection_up(&mut self) {
        let options = Self::get_theme_options();
        if self.selected_index == 0 {
            self.selected_index = options.len() - 1;
        } else {
            self.selected_index -= 1;
        }
        self.current_theme = options[self.selected_index].0;
        // Live preview - update theme immediately (no history event)
        self.app_event_tx.send(AppEvent::PreviewTheme(self.current_theme));
    }

    fn move_selection_down(&mut self) {
        let options = Self::get_theme_options();
        self.selected_index = (self.selected_index + 1) % options.len();
        self.current_theme = options[self.selected_index].0;
        // Live preview - update theme immediately (no history event)
        self.app_event_tx.send(AppEvent::PreviewTheme(self.current_theme));
    }

    fn confirm_selection(&self) {
        // Confirm the selection - this will add it to history
        self.app_event_tx.send(AppEvent::UpdateTheme(self.current_theme));
    }
    
    fn cancel_selection(&mut self) {
        // Restore original theme on cancel (no history event)
        if self.current_theme != self.original_theme {
            self.app_event_tx.send(AppEvent::PreviewTheme(self.original_theme));
        }
    }
}

impl<'a> BottomPaneView<'a> for ThemeSelectionView {
    fn desired_height(&self, _width: u16) -> u16 {
        // Use most of the available screen for better scrolling
        // But cap it at the number of themes + header/footer
        let theme_count = Self::get_theme_options().len() as u16;
        (theme_count + 4).min(20)
    }
    
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.move_selection_up();
            }
            KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.move_selection_down();
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.confirm_selection();
                self.is_complete = true;
            }
            KeyEvent {
                code: KeyCode::Esc,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.cancel_selection();
                self.is_complete = true;
            }
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let options = Self::get_theme_options();
        let theme = crate::theme::current_theme();
        
        // Calculate available height for the list (excluding header and footer)
        let available_height = area.height.saturating_sub(4) as usize;
        
        // Calculate scroll offset to keep selected item visible
        let scroll_offset = if available_height >= options.len() {
            // All items fit, no scrolling needed
            0
        } else if self.selected_index < available_height / 2 {
            // Near the top
            0
        } else if self.selected_index >= options.len() - available_height / 2 {
            // Near the bottom
            options.len().saturating_sub(available_height)
        } else {
            // Center the selected item
            self.selected_index.saturating_sub(available_height / 2)
        };
        
        // Create content
        let mut lines = vec![
            Line::from(vec![
                Span::styled(
                    "Theme Selection",
                    Style::default()
                        .fg(theme.text_bright)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
        ];

        // Add visible themes based on scroll offset
        let visible_end = (scroll_offset + available_height).min(options.len());
        for i in scroll_offset..visible_end {
            let (theme_enum, name, description) = &options[i];
            let is_selected = i == self.selected_index;
            let is_original = *theme_enum == self.original_theme;
            
            let prefix = if is_selected { "▶ " } else { "  " };
            let suffix = if is_original { " (original)" } else { "" };
            
            let mut spans = vec![
                Span::raw(prefix),
            ];
            
            if is_selected {
                spans.push(Span::styled(
                    *name,
                    Style::default()
                        .fg(theme.primary)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::styled(
                    *name,
                    Style::default().fg(theme.text),
                ));
            }
            
            spans.push(Span::styled(
                suffix,
                Style::default().fg(theme.text_dim),
            ));
            
            if !suffix.is_empty() {
                spans.push(Span::raw(" "));
            } else {
                spans.push(Span::raw("  "));
            }
            
            spans.push(Span::styled(
                *description,
                Style::default().fg(theme.text_dim),
            ));
            
            lines.push(Line::from(spans));
        }
        
        // Add scroll indicators if needed
        if scroll_offset > 0 || visible_end < options.len() {
            lines.push(Line::from(""));
            let scroll_info = format!(
                "[{}/{}]",
                self.selected_index + 1,
                options.len()
            );
            lines.push(Line::from(vec![
                Span::styled(
                    scroll_info,
                    Style::default().fg(theme.text_dim),
                ),
            ]));
        } else {
            lines.push(Line::from(""));
        }
        
        lines.push(Line::from(vec![
            Span::styled("↑↓", Style::default().fg(theme.keyword).add_modifier(Modifier::BOLD)),
            Span::styled(" preview • ", Style::default().fg(theme.text_dim)),
            Span::styled("Enter", Style::default().fg(theme.keyword).add_modifier(Modifier::BOLD)),
            Span::styled(" confirm • ", Style::default().fg(theme.text_dim)),
            Span::styled("Esc", Style::default().fg(theme.keyword).add_modifier(Modifier::BOLD)),
            Span::styled(" cancel", Style::default().fg(theme.text_dim)),
        ]));

        // Use full width for better integration
        let render_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height.min(lines.len() as u16 + 2),
        };

        // Clear the area with theme background
        for y in render_area.y..render_area.y + render_area.height {
            for x in render_area.x..render_area.x + render_area.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(Style::default().bg(theme.background));
                }
            }
        }

        // Render with themed border
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_focused))
            .style(Style::default()
                .bg(theme.background)
                .fg(theme.text));

        let paragraph = Paragraph::new(lines)
            .block(block)
            .alignment(Alignment::Left);

        paragraph.render(render_area, buf);
    }
}