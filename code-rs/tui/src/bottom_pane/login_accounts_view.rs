use std::cell::RefCell;
use std::io;
use std::path::PathBuf;
use std::rc::{Rc, Weak};

use chrono::{DateTime, Utc};
use code_core::auth;
use code_core::auth_accounts::{self, StoredAccount};
use code_login::AuthMode;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap};
use textwrap::Options as TwOptions;

use crate::account_label::{account_display_label, account_mode_priority};
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::chatwidget::BackgroundOrderTicket;

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::form_text_field::FormTextField;
use super::BottomPane;

/// Interactive view shown for `/login` to manage stored accounts.
pub(crate) struct LoginAccountsView {
    state: Rc<RefCell<LoginAccountsState>>,
}

impl LoginAccountsView {
    pub fn new(
        code_home: PathBuf,
        app_event_tx: AppEventSender,
        tail_ticket: BackgroundOrderTicket,
    ) -> (Self, Rc<RefCell<LoginAccountsState>>) {
        let state = Rc::new(RefCell::new(LoginAccountsState::new(
            code_home,
            app_event_tx,
            tail_ticket,
        )));
        (Self { state: state.clone() }, state)
    }
}

impl<'a> BottomPaneView<'a> for LoginAccountsView {
    fn handle_key_event(&mut self, pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let mut state = self.state.borrow_mut();
        state.handle_key_event(key_event);
        if state.should_close() {
            state.set_complete();
        }
        pane.request_redraw();
    }

    fn is_complete(&self) -> bool {
        self.state.borrow().is_complete
    }

    fn desired_height(&self, width: u16) -> u16 {
        let state = self.state.borrow();
        state.desired_height(width)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let state = self.state.borrow();
        state.render(area, buf);
    }

    fn handle_paste(&mut self, text: String) -> ConditionalUpdate {
        let mut state = self.state.borrow_mut();
        state.handle_paste(text)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AccountRow {
    id: String,
    label: String,
    detail: Option<String>,
    mode: AuthMode,
    is_active: bool,
}

#[derive(Clone, Debug)]
struct Feedback {
    message: String,
    is_error: bool,
}

#[derive(Debug)]
enum ViewMode {
    List,
    ConfirmRemove { account_id: String },
}

pub(crate) struct LoginAccountsState {
    code_home: PathBuf,
    app_event_tx: AppEventSender,
    tail_ticket: BackgroundOrderTicket,
    accounts: Vec<AccountRow>,
    active_account_id: Option<String>,
    selected: usize,
    mode: ViewMode,
    feedback: Option<Feedback>,
    is_complete: bool,
}

impl LoginAccountsState {
    fn new(
        code_home: PathBuf,
        app_event_tx: AppEventSender,
        tail_ticket: BackgroundOrderTicket,
    ) -> Self {
        let mut state = Self {
            code_home,
            app_event_tx,
            tail_ticket,
            accounts: Vec::new(),
            active_account_id: None,
            selected: 0,
            mode: ViewMode::List,
            feedback: None,
            is_complete: false,
        };
        state.sync_account_store_from_auth();
        state.reload_accounts();
        state
    }

    fn send_tail(&self, message: impl Into<String>) {
        self.app_event_tx
            .send_background_event_with_ticket(&self.tail_ticket, message);
    }

    pub fn weak_handle(state: &Rc<RefCell<Self>>) -> Weak<RefCell<Self>> {
        Rc::downgrade(state)
    }

    fn reload_accounts(&mut self) {
        let previously_selected_id = self
            .accounts
            .get(self.selected)
            .map(|row| row.id.clone());

        match auth_accounts::list_accounts(&self.code_home) {
            Ok(raw_accounts) => {
                let active_id = auth_accounts::get_active_account_id(&self.code_home).ok().flatten();
                self.active_account_id = active_id.clone();
                self.accounts = raw_accounts
                    .into_iter()
                    .map(|account| AccountRow::from(account, active_id.as_deref()))
                    .collect();

                self.accounts.sort_by(|a, b| {
                    let priority = account_mode_priority;
                    let a_priority = priority(a.mode);
                    let b_priority = priority(b.mode);
                    a_priority
                        .cmp(&b_priority)
                        .then_with(|| a.label.to_ascii_lowercase().cmp(&b.label.to_ascii_lowercase()))
                        .then_with(|| a.label.cmp(&b.label))
                        .then_with(|| a.id.cmp(&b.id))
                });

                let mut selected_idx = previously_selected_id
                    .and_then(|id| self.accounts.iter().position(|row| row.id == id))
                    .or_else(|| {
                        active_id
                            .as_ref()
                            .and_then(|id| self.accounts.iter().position(|row| &row.id == id))
                    });

                if self.accounts.is_empty() {
                    self.selected = 0;
                } else {
                    if selected_idx.is_none() {
                        selected_idx = Some(0);
                    }
                    self.selected = selected_idx.unwrap_or(0).min(self.accounts.len() - 1);
                }
            }
            Err(err) => {
                self.feedback = Some(Feedback {
                    message: format!("Failed to read accounts: {err}"),
                    is_error: true,
                });
                self.accounts.clear();
                self.selected = 0;
                self.active_account_id = None;
            }
        }
    }

    fn sync_account_store_from_auth(&mut self) {
        let auth_file = auth::get_auth_file(&self.code_home);
        let auth_json = match auth::try_read_auth_json(&auth_file) {
            Ok(auth) => auth,
            Err(err) => {
                if err.kind() != io::ErrorKind::NotFound {
                    self.feedback = Some(Feedback {
                        message: format!("Failed to read current auth: {err}"),
                        is_error: true,
                    });
                }
                return;
            }
        };

        if let Some(tokens) = auth_json.tokens.clone() {
            let last_refresh = auth_json.last_refresh.unwrap_or_else(Utc::now);
            let email = tokens.id_token.email.clone();
            if let Err(err) = auth_accounts::upsert_chatgpt_account(
                &self.code_home,
                tokens,
                last_refresh,
                email,
                true,
            ) {
                self.feedback = Some(Feedback {
                    message: format!("Failed to record ChatGPT login: {err}"),
                    is_error: true,
                });
            }
            return;
        }

        if let Some(api_key) = auth_json.openai_api_key.as_ref() {
            if let Err(err) = auth_accounts::upsert_api_key_account(
                &self.code_home,
                api_key.clone(),
                None,
                true,
            ) {
                self.feedback = Some(Feedback {
                    message: format!("Failed to record API key login: {err}"),
                    is_error: true,
                });
            }
        }
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        let mode = std::mem::replace(&mut self.mode, ViewMode::List);
        match mode {
            ViewMode::List => {
                self.mode = ViewMode::List;
                self.handle_list_key(key_event);
            }
            ViewMode::ConfirmRemove { account_id } => {
                self.mode = ViewMode::ConfirmRemove { account_id };
                self.handle_confirm_remove_key(key_event);
            }
        }
    }

    fn handle_list_key(&mut self, key_event: KeyEvent) {
        let account_count = self.accounts.len();

        match key_event.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.is_complete = true;
            }
            KeyCode::Up => {
                if account_count == 0 {
                    self.selected = 0;
                } else if self.selected == 0 {
                    self.selected = account_count;
                } else {
                    self.selected -= 1;
                }
            }
            KeyCode::Down => {
                if account_count == 0 {
                    self.selected = 0;
                } else if self.selected >= account_count {
                    self.selected = 0;
                } else {
                    self.selected = (self.selected + 1).min(account_count);
                }
            }
            KeyCode::Char('d') => {
                if self.selected < account_count {
                    if let Some(account) = self.accounts.get(self.selected) {
                        self.mode = ViewMode::ConfirmRemove { account_id: account.id.clone() };
                    }
                }
            }
            KeyCode::Char('r') => {
                self.reload_accounts();
            }
            KeyCode::Enter => {
                if self.selected < account_count {
                    if let Some(account) = self.accounts.get(self.selected) {
                        let label = account.label.clone();
                        let mode = account.mode;
                        if self.activate_account(account.id.clone(), mode) {
                            self.mode = ViewMode::List;
                            self.send_tail(format!("Switched to {label}"));
                            self.is_complete = true;
                        }
                    }
                } else {
                    self.is_complete = true;
                    self.app_event_tx.send(AppEvent::ShowLoginAddAccount);
                }
            }
            _ => {}
        }
    }

    fn handle_confirm_remove_key(&mut self, key_event: KeyEvent) {
        let account_id = if let ViewMode::ConfirmRemove { account_id } = &self.mode {
            account_id.clone()
        } else {
            return;
        };

        match key_event.code {
            KeyCode::Esc | KeyCode::Char('n') => {
                self.mode = ViewMode::List;
            }
            KeyCode::Enter | KeyCode::Char('y') => {
                self.remove_account(account_id);
            }
            _ => {}
        }
    }

    fn activate_account(&mut self, account_id: String, mode: AuthMode) -> bool {
        match auth::activate_account(&self.code_home, &account_id) {
            Ok(()) => {
                self.feedback = Some(Feedback {
                    message: match mode {
                        AuthMode::ChatGPT => "ChatGPT account selected".to_string(),
                        AuthMode::ApiKey => "API key selected".to_string(),
                    },
                    is_error: false,
                });
                self.reload_accounts();
                self.app_event_tx
                    .send(AppEvent::LoginUsingChatGptChanged { using_chatgpt_auth: mode == AuthMode::ChatGPT });
                true
            }
            Err(err) => {
                self.feedback = Some(Feedback {
                    message: format!("Failed to activate account: {err}"),
                    is_error: true,
                });
                false
            }
        }
    }

    fn remove_account(&mut self, account_id: String) {
        match auth_accounts::remove_account(&self.code_home, &account_id) {
            Ok(Some(_)) => {
                let removed_active = self
                    .active_account_id
                    .as_ref()
                    .is_some_and(|id| id == &account_id);
                if removed_active {
                    let _ = auth::logout(&self.code_home);
                }
                self.feedback = Some(Feedback {
                    message: "Account disconnected".to_string(),
                    is_error: false,
                });
                self.mode = ViewMode::List;
                self.reload_accounts();
                let using_chatgpt = self
                    .active_account_id
                    .as_ref()
                    .and_then(|id| auth_accounts::find_account(&self.code_home, id).ok().flatten())
                    .map(|acc| acc.mode == AuthMode::ChatGPT)
                    .unwrap_or(false);
                self.app_event_tx
                    .send(AppEvent::LoginUsingChatGptChanged { using_chatgpt_auth: using_chatgpt });
            }
            Ok(None) => {
                self.feedback = Some(Feedback {
                    message: "Account no longer exists".to_string(),
                    is_error: true,
                });
                self.mode = ViewMode::List;
                self.reload_accounts();
            }
            Err(err) => {
                self.feedback = Some(Feedback {
                    message: format!("Failed to remove account: {err}"),
                    is_error: true,
                });
                self.mode = ViewMode::List;
            }
        }
    }

    fn handle_paste(&mut self, _text: String) -> ConditionalUpdate {
        ConditionalUpdate::NoRedraw
    }

    fn desired_height(&self, _width: u16) -> u16 {
        const MIN_HEIGHT: usize = 9;
        let content_lines = self.content_line_count();
        let total = content_lines + 2; // account for top/bottom borders
        total.max(MIN_HEIGHT) as u16
    }

    fn content_line_count(&self) -> usize {
        let mut lines = 0usize;

        if self.feedback.is_some() {
            lines += 2; // message + blank spacer
        }

        lines += 2; // heading + blank spacer after heading

        if self.accounts.is_empty() {
            lines += 1;
        } else {
            lines += self.accounts.len();
        }

        lines += 1; // blank before add row
        lines += 1; // add account row
        lines += 2; // blank + key hints row

        if matches!(self.mode, ViewMode::ConfirmRemove { .. }) {
            lines += 3; // blank, question, instruction
        }

        lines
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title(" Manage Accounts ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let mut lines = Vec::new();
        if let Some(feedback) = &self.feedback {
            let style = if feedback.is_error {
                Style::default().fg(crate::colors::error()).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(crate::colors::success()).add_modifier(Modifier::BOLD)
            };
            lines.push(Line::from(vec![Span::styled(feedback.message.clone(), style)]));
            lines.push(Line::from(""));
        }

        lines.push(Line::from(vec![Span::styled(
            "Connected Accounts",
            Style::default().add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(""));

        if self.accounts.is_empty() {
            lines.push(Line::from(Span::styled(
                "No accounts connected yet.",
                Style::default().fg(crate::colors::text_dim()),
            )));
        } else {
            for (idx, account) in self.accounts.iter().enumerate() {
                let selected = idx == self.selected;
                let arrow_style = if selected {
                    Style::default().fg(crate::colors::primary())
                } else {
                    Style::default().fg(crate::colors::text_dim())
                };
                let label_style = if selected {
                    Style::default()
                        .fg(crate::colors::primary())
                        .add_modifier(Modifier::BOLD)
                } else if account.is_active {
                    Style::default()
                        .fg(crate::colors::success())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let mut spans = vec![
                    Span::styled(if selected { "› " } else { "  " }, arrow_style),
                    Span::styled(account.label.clone(), label_style),
                ];

                if account.is_active {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        "(current)",
                        Style::default()
                            .fg(crate::colors::success())
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                if let Some(detail) = &account.detail {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(
                        detail.clone(),
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                }

                lines.push(Line::from(spans));
            }
        }

        let add_index = self.accounts.len();
        let add_selected = self.selected == add_index;
        let add_arrow_style = if add_selected {
            Style::default().fg(crate::colors::primary())
        } else {
            Style::default().fg(crate::colors::text_dim())
        };
        let add_label_style = if add_selected {
            Style::default()
                .fg(crate::colors::primary())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(crate::colors::text())
        };

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(if add_selected { "› " } else { "  " }, add_arrow_style),
            Span::styled("Add account…", add_label_style),
        ]));

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("↑↓", Style::default().fg(crate::colors::function())),
            Span::styled(" Navigate  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("Enter", Style::default().fg(crate::colors::success())),
            Span::styled(" Select  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("d", Style::default().fg(crate::colors::warning()).add_modifier(Modifier::BOLD)),
            Span::styled(" Disconnect  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("Esc", Style::default().fg(crate::colors::error()).add_modifier(Modifier::BOLD)),
            Span::styled(" Close", Style::default().fg(crate::colors::text_dim())),
        ]));

        if matches!(self.mode, ViewMode::ConfirmRemove { .. }) {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![Span::styled("Confirm removal?", Style::default().add_modifier(Modifier::BOLD))]));
            lines.push(Line::from("Press Enter to disconnect or Esc to cancel."));
        }

        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Left)
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .render(
                Rect {
                    x: inner.x.saturating_add(1),
                    y: inner.y,
                    width: inner.width.saturating_sub(2),
                    height: inner.height,
                },
                buf,
            );
    }

    fn should_close(&self) -> bool {
        self.is_complete
    }

    fn set_complete(&mut self) {
        self.is_complete = true;
    }
}

fn wrap_url_segments(url: &str, available_width: u16) -> Vec<String> {
    let width = available_width.max(1) as usize;
    let mut opts = TwOptions::new(width);
    opts.break_words = true;
    textwrap::wrap(url, opts)
        .into_iter()
        .map(|segment| segment.into_owned())
        .collect()
}

pub(crate) struct LoginAddAccountView {
    state: Rc<RefCell<LoginAddAccountState>>,
}

impl LoginAddAccountView {
    pub fn new(
        code_home: PathBuf,
        app_event_tx: AppEventSender,
        tail_ticket: BackgroundOrderTicket,
    ) -> (Self, Rc<RefCell<LoginAddAccountState>>) {
        let state = Rc::new(RefCell::new(LoginAddAccountState::new(
            code_home,
            app_event_tx,
            tail_ticket,
        )));
        (Self { state: state.clone() }, state)
    }
}

impl<'a> BottomPaneView<'a> for LoginAddAccountView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        self.state.borrow_mut().handle_key_event(key_event);
    }

    fn is_complete(&self) -> bool {
        self.state.borrow().is_complete()
    }

    fn desired_height(&self, _width: u16) -> u16 {
        self.state.borrow().desired_height() as u16
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.state.borrow().render(area, buf);
    }

    fn handle_paste(&mut self, text: String) -> ConditionalUpdate {
        self.state.borrow_mut().handle_paste(text)
    }
}

#[derive(Debug)]
enum AddStep {
    Choose { selected: usize },
    ApiKey { field: FormTextField },
    Waiting { auth_url: Option<String> },
    DeviceCode(DeviceCodeState),
}

#[derive(Debug)]
struct DeviceCodeState {
    authorize_url: Option<String>,
    user_code: Option<String>,
    status: DeviceCodeStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceCodeStatus {
    Generating,
    WaitingForApproval,
}

pub(crate) struct LoginAddAccountState {
    code_home: PathBuf,
    app_event_tx: AppEventSender,
    tail_ticket: BackgroundOrderTicket,
    step: AddStep,
    feedback: Option<Feedback>,
    is_complete: bool,
}

impl LoginAddAccountState {
    fn new(
        code_home: PathBuf,
        app_event_tx: AppEventSender,
        tail_ticket: BackgroundOrderTicket,
    ) -> Self {
        Self {
            code_home,
            app_event_tx,
            tail_ticket,
            step: AddStep::Choose { selected: 0 },
            feedback: None,
            is_complete: false,
        }
    }

    fn send_tail(&self, message: impl Into<String>) {
        self.app_event_tx
            .send_background_event_with_ticket(&self.tail_ticket, message);
    }

    pub fn weak_handle(state: &Rc<RefCell<Self>>) -> Weak<RefCell<Self>> {
        Rc::downgrade(state)
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match &mut self.step {
            AddStep::Choose { selected } => match key_event.code {
                KeyCode::Esc => {
                    self.finish_and_show_accounts();
                }
                KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => {
                    *selected = if *selected == 0 { 1 } else { 0 };
                }
                KeyCode::Enter => {
                    if *selected == 0 {
                        self.feedback = Some(Feedback {
                            message: "Opening browser for ChatGPT sign-in…".to_string(),
                            is_error: false,
                        });
                        self.step = AddStep::Waiting { auth_url: None };
                        self.app_event_tx.send(AppEvent::LoginStartChatGpt);
                    } else {
                        self.feedback = None;
                        self.step = AddStep::ApiKey { field: FormTextField::new_single_line() };
                    }
                }
                _ => {}
            },
            AddStep::ApiKey { field } => match key_event.code {
                KeyCode::Esc => {
                    self.finish_and_show_accounts();
                }
                KeyCode::Enter => {
                    let key = field.text().trim().to_string();
                    if key.is_empty() {
                        self.feedback = Some(Feedback {
                            message: "API key cannot be empty".to_string(),
                            is_error: true,
                        });
                    } else {
                        match auth::login_with_api_key(&self.code_home, &key) {
                            Ok(()) => {
                                self.feedback = Some(Feedback {
                                    message: "API key connected".to_string(),
                                    is_error: false,
                                });
                                self.send_tail("Added API key account".to_string());
                                self.app_event_tx
                                    .send(AppEvent::LoginUsingChatGptChanged { using_chatgpt_auth: false });
                                self.finish_and_show_accounts();
                            }
                            Err(err) => {
                                self.feedback = Some(Feedback {
                                    message: format!("Failed to store API key: {err}"),
                                    is_error: true,
                                });
                            }
                        }
                    }
                }
                _ => {
                    let _ = field.handle_key(key_event);
                }
            },
            AddStep::Waiting { .. } => match key_event.code {
                KeyCode::Esc => {
                    self.app_event_tx.send(AppEvent::LoginCancelChatGpt);
                }
                KeyCode::Char('c') | KeyCode::Char('C') => {
                    self.feedback = Some(Feedback {
                        message: "Switching to code authentication…".to_string(),
                        is_error: false,
                    });
                    self.step = AddStep::DeviceCode(DeviceCodeState::generating());
                    self.app_event_tx.send(AppEvent::LoginStartDeviceCode);
                }
                _ => {}
            },
            AddStep::DeviceCode(_) => {
                if matches!(key_event.code, KeyCode::Esc) {
                    self.app_event_tx.send(AppEvent::LoginCancelChatGpt);
                }
            }
        }
    }

    fn handle_paste(&mut self, text: String) -> ConditionalUpdate {
        if let AddStep::ApiKey { field } = &mut self.step {
            let _ = field.handle_paste(text);
            ConditionalUpdate::NeedsRedraw
        } else {
            ConditionalUpdate::NoRedraw
        }
    }

    fn desired_height(&self) -> usize {
        let mut lines = 5; // title + spacing baseline
        if self.feedback.is_some() {
            lines += 2;
        }

        match &self.step {
            AddStep::Choose { .. } => {
                lines += 4; // options + spacing
            }
            AddStep::ApiKey { .. } => {
                lines += 4; // instructions + input + spacing
            }
            AddStep::Waiting { auth_url } => {
                lines += 3; // instructions + cancel hint
                if auth_url.is_some() {
                    lines += 1;
                }
            }
            AddStep::DeviceCode(_) => {
                lines += 6;
            }
        }

        lines.max(10) + 2
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title(" Add Account ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let content_width = inner.width.saturating_sub(2).max(1);
        let content_area = Rect {
            x: inner.x.saturating_add(1),
            y: inner.y,
            width: content_width,
            height: inner.height,
        };

        let mut lines = Vec::new();
        if let Some(feedback) = &self.feedback {
            let style = if feedback.is_error {
                Style::default().fg(crate::colors::error()).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(crate::colors::success()).add_modifier(Modifier::BOLD)
            };
            lines.push(Line::from(vec![Span::styled(feedback.message.clone(), style)]));
            lines.push(Line::from(""));
        }

        match &self.step {
            AddStep::Choose { selected } => {
                lines.push(Line::from("Choose how you’d like to add an account:"));
                lines.push(Line::from(""));
                let options = ["ChatGPT sign-in", "API key"];
                for (idx, option) in options.iter().enumerate() {
                    let mut spans = Vec::new();
                    if idx == *selected {
                        spans.push(Span::styled("› ", Style::default().fg(crate::colors::primary())));
                        spans.push(
                            Span::styled((*option).to_string(), Style::default().fg(crate::colors::primary()).add_modifier(Modifier::BOLD)),
                        );
                    } else {
                        spans.push(Span::raw("  "));
                        spans.push(Span::styled((*option).to_string(), Style::default().fg(crate::colors::text_dim())));
                    }
                    lines.push(Line::from(spans));
                }
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("↑↓", Style::default().fg(crate::colors::function())),
                    Span::styled(" Navigate  ", Style::default().fg(crate::colors::text_dim())),
                    Span::styled("Enter", Style::default().fg(crate::colors::success())),
                    Span::styled(" Select  ", Style::default().fg(crate::colors::text_dim())),
                    Span::styled("Esc", Style::default().fg(crate::colors::error()).add_modifier(Modifier::BOLD)),
                    Span::styled(" Back", Style::default().fg(crate::colors::text_dim())),
                ]));
            }
            AddStep::ApiKey { field } => {
                lines.push(Line::from("Paste your OpenAI API key:"));
                lines.push(Line::from(field.render_line()));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("Enter", Style::default().fg(crate::colors::success())),
                    Span::styled(" Save  ", Style::default().fg(crate::colors::text_dim())),
                    Span::styled("Esc", Style::default().fg(crate::colors::error()).add_modifier(Modifier::BOLD)),
                    Span::styled(" Cancel", Style::default().fg(crate::colors::text_dim())),
                ]));
            }
            AddStep::Waiting { auth_url } => {
                lines.push(Line::from("Finish signing in with ChatGPT in your browser."));
                lines.push(Line::from(vec![
                    Span::styled("Not seeing a browser? ", Style::default().fg(crate::colors::text_dim())),
                    Span::styled(
                        "Press C to switch to code authentication.",
                        Style::default().fg(crate::colors::primary()),
                    ),
                ]));
                if let Some(url) = auth_url {
                    for chunk in wrap_url_segments(url, content_width) {
                        lines.push(Line::from(vec![Span::styled(
                            chunk,
                            Style::default().fg(crate::colors::primary()),
                        )]));
                    }
                }
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("Esc", Style::default().fg(crate::colors::error()).add_modifier(Modifier::BOLD)),
                    Span::styled(" Cancel login", Style::default().fg(crate::colors::text_dim())),
                ]));
            }
            AddStep::DeviceCode(state) => {
                lines.push(Line::from("Complete sign-in using a verification code."));
                match state.status {
                    DeviceCodeStatus::Generating => {
                        lines.push(Line::from("Generating a secure code and link…"));
                    }
                    DeviceCodeStatus::WaitingForApproval => {
                        if let Some(code) = &state.user_code {
                            lines.push(Line::from(vec![
                                Span::styled("Code: ", Style::default().fg(crate::colors::text_dim())),
                                Span::styled(
                                    code.clone(),
                                    Style::default().fg(crate::colors::primary()).add_modifier(Modifier::BOLD),
                                ),
                            ]));
                        }
                        if let Some(url) = &state.authorize_url {
                            lines.push(Line::from("Visit this link on any device:"));
                            for chunk in wrap_url_segments(url, content_width) {
                                lines.push(Line::from(vec![Span::styled(
                                    chunk,
                                    Style::default().fg(crate::colors::info()),
                                )]));
                            }
                        }
                        lines.push(Line::from("Keep this code private. It expires after 15 minutes."));
                    }
                }
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("Esc", Style::default().fg(crate::colors::error()).add_modifier(Modifier::BOLD)),
                    Span::styled(" Cancel login", Style::default().fg(crate::colors::text_dim())),
                ]));
            }
        }

        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Left)
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .render(content_area, buf);
    }

    pub fn acknowledge_chatgpt_started(&mut self, auth_url: String) {
        self.step = AddStep::Waiting { auth_url: Some(auth_url) };
        self.feedback = Some(Feedback {
            message: "Browser opened. Complete sign-in to finish.".to_string(),
            is_error: false,
        });
    }

    pub fn acknowledge_chatgpt_failed(&mut self, error: String) {
        self.step = AddStep::Choose { selected: 0 };
        self.feedback = Some(Feedback { message: error, is_error: true });
    }

    pub fn begin_device_code_flow(&mut self) {
        if !matches!(self.step, AddStep::DeviceCode(_)) {
            self.step = AddStep::DeviceCode(DeviceCodeState::generating());
        }
        self.feedback = Some(Feedback {
            message: "Use the on-screen code to finish signing in.".to_string(),
            is_error: false,
        });
    }

    pub fn set_device_code_ready(&mut self, authorize_url: String, user_code: String) {
        self.step = AddStep::DeviceCode(DeviceCodeState::with_details(authorize_url, user_code));
        self.feedback = Some(Feedback {
            message: "Enter the code in your browser to continue.".to_string(),
            is_error: false,
        });
    }

    pub fn on_device_code_failed(&mut self, error: String) {
        self.step = AddStep::Choose { selected: 0 };
        self.feedback = Some(Feedback { message: error, is_error: true });
    }

    pub fn on_chatgpt_complete(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
        self.feedback = Some(Feedback { message: "ChatGPT account connected".to_string(), is_error: false });
        self.send_tail("ChatGPT account connected".to_string());
                self.app_event_tx
                    .send(AppEvent::LoginUsingChatGptChanged { using_chatgpt_auth: true });
                self.finish_and_show_accounts();
            }
            Err(err) => {
                self.step = AddStep::Choose { selected: 0 };
                self.feedback = Some(Feedback { message: err, is_error: true });
            }
        }
    }

    pub fn cancel_active_flow(&mut self) {
        let message = match self.step {
            AddStep::DeviceCode(_) => "Cancelled code authentication",
            AddStep::Waiting { .. } => "Cancelled ChatGPT login",
            _ => "Cancelled login",
        };
        self.step = AddStep::Choose { selected: 0 };
        self.feedback = Some(Feedback { message: message.to_string(), is_error: false });
    }

    fn finish_and_show_accounts(&mut self) {
        self.is_complete = true;
        self.app_event_tx.send(AppEvent::ShowLoginAccounts);
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }
}

impl DeviceCodeState {
    fn generating() -> Self {
        Self {
            authorize_url: None,
            user_code: None,
            status: DeviceCodeStatus::Generating,
        }
    }

    fn with_details(authorize_url: String, user_code: String) -> Self {
        Self {
            authorize_url: Some(authorize_url),
            user_code: Some(user_code),
            status: DeviceCodeStatus::WaitingForApproval,
        }
    }
}

impl AccountRow {
    fn from(account: StoredAccount, active_id: Option<&str>) -> Self {
        let id = account.id.clone();
        let label = account_display_label(&account);
        let mode = account.mode;
        let mut detail_parts: Vec<String> = Vec::new();

        if let AuthMode::ChatGPT = mode {
            if let Some(plan) = account
                .tokens
                .as_ref()
                .and_then(|t| t.id_token.get_chatgpt_plan_type())
            {
                detail_parts.push(format!("{plan} Plan"));
            }
        }

        if let Some(created_at) = account.created_at {
            detail_parts.push(format!("connected {}", format_timestamp(created_at)));
        }

        let detail = if detail_parts.is_empty() {
            None
        } else {
            Some(detail_parts.join(" • "))
        };

        let is_active = active_id.is_some_and(|candidate| candidate == id);

        Self {
            id,
            label,
            detail,
            mode,
            is_active,
        }
    }
}

fn format_timestamp(ts: DateTime<Utc>) -> String {
    ts.with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

impl FormTextField {
    fn render_line(&self) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::raw(self.text().to_string()));
        spans.push(Span::raw("_"));
        Line::from(spans)
    }
}
