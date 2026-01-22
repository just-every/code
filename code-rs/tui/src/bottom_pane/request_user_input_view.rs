use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap};

use code_protocol::request_user_input::RequestUserInputAnswer;
use code_protocol::request_user_input::RequestUserInputQuestion;
use code_protocol::request_user_input::RequestUserInputResponse;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

use super::bottom_pane_view::BottomPaneView;
use super::scroll_state::ScrollState;
use super::selection_popup_common::GenericDisplayRow;
use super::selection_popup_common::render_rows;
use super::{BottomPane, CancellationEvent};

#[derive(Debug, Clone)]
struct AnswerState {
    option_state: ScrollState,
    freeform: String,
}

pub(crate) struct RequestUserInputView {
    app_event_tx: AppEventSender,
    turn_id: String,
    questions: Vec<RequestUserInputQuestion>,
    answers: Vec<AnswerState>,
    current_idx: usize,
    submitting: bool,
    complete: bool,
}

impl RequestUserInputView {
    pub(crate) fn new(
        turn_id: String,
        questions: Vec<RequestUserInputQuestion>,
        app_event_tx: AppEventSender,
    ) -> Self {
        let answers = questions
            .iter()
            .map(|q| {
                let mut option_state = ScrollState::new();
                if q
                    .options
                    .as_ref()
                    .is_some_and(|options| !options.is_empty())
                {
                    option_state.selected_idx = Some(0);
                }
                AnswerState {
                    option_state,
                    freeform: String::new(),
                }
            })
            .collect();

        Self {
            app_event_tx,
            turn_id,
            questions,
            answers,
            current_idx: 0,
            submitting: false,
            complete: false,
        }
    }

    fn question_count(&self) -> usize {
        self.questions.len()
    }

    fn current_question(&self) -> Option<&RequestUserInputQuestion> {
        self.questions.get(self.current_idx)
    }

    fn current_answer_mut(&mut self) -> Option<&mut AnswerState> {
        self.answers.get_mut(self.current_idx)
    }

    fn current_answer(&self) -> Option<&AnswerState> {
        self.answers.get(self.current_idx)
    }

    fn current_options_len(&self) -> usize {
        self.current_question()
            .and_then(|q| q.options.as_ref())
            .map(std::vec::Vec::len)
            .unwrap_or(0)
    }

    fn current_has_options(&self) -> bool {
        self.current_options_len() > 0
    }

    fn move_selection(&mut self, up: bool) {
        let options_len = self.current_options_len();
        if options_len == 0 {
            return;
        }
        let Some(answer) = self.current_answer_mut() else {
            return;
        };
        if up {
            answer.option_state.move_up_wrap(options_len);
        } else {
            answer.option_state.move_down_wrap(options_len);
        }
    }

    fn push_freeform_char(&mut self, ch: char) {
        let Some(answer) = self.current_answer_mut() else {
            return;
        };
        answer.freeform.push(ch);
    }

    fn pop_freeform_char(&mut self) {
        let Some(answer) = self.current_answer_mut() else {
            return;
        };
        let _ = answer.freeform.pop();
    }

    fn go_next_or_submit(&mut self) {
        if self.question_count() == 0 {
            self.complete = true;
            return;
        }

        if self.current_idx + 1 >= self.question_count() {
            self.submit();
        } else {
            self.current_idx = self.current_idx.saturating_add(1);
        }
    }

    fn submit(&mut self) {
        if self.submitting {
            return;
        }

        let mut answers = HashMap::new();
        for (idx, question) in self.questions.iter().enumerate() {
            let Some(answer_state) = self.answers.get(idx) else {
                continue;
            };
            let options = question.options.as_ref().filter(|opts| !opts.is_empty());
            let mut answer_list = Vec::new();

            if let Some(options) = options {
                let selected_idx = answer_state.option_state.selected_idx;
                if let Some(label) = selected_idx
                    .and_then(|i| options.get(i))
                    .map(|opt| opt.label.clone())
                {
                    answer_list.push(label);
                }
            } else {
                let value = answer_state.freeform.trim_end();
                // Preserve the legacy behavior of `request_user_input` composer replies:
                // always provide an answer slot, even when empty.
                answer_list.push(value.to_string());
            }

            answers.insert(
                question.id.clone(),
                RequestUserInputAnswer {
                    answers: answer_list,
                },
            );
        }

        self.app_event_tx.send(AppEvent::RequestUserInputAnswer {
            turn_id: self.turn_id.clone(),
            response: RequestUserInputResponse { answers },
        });
        // Keep the picker visible until the ChatWidget consumes the answer.
        // This prevents a race where the composer becomes active while
        // `pending_request_user_input` is still set.
        self.submitting = true;
    }
}

impl BottomPaneView<'_> for RequestUserInputView {
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn handle_key_event(&mut self, _pane: &mut BottomPane<'_>, key_event: KeyEvent) {
        if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return;
        }

        if self.submitting {
            return;
        }

        match key_event.code {
            KeyCode::Esc => {
                // Close this UI and fall back to the composer for manual input.
                self.complete = true;
            }
            KeyCode::Enter => {
                self.go_next_or_submit();
            }
            KeyCode::PageUp => {
                self.current_idx = self.current_idx.saturating_sub(1);
            }
            KeyCode::PageDown => {
                self.current_idx =
                    (self.current_idx + 1).min(self.question_count().saturating_sub(1));
            }
            KeyCode::Up => {
                if self.current_has_options() {
                    self.move_selection(true);
                }
            }
            KeyCode::Down => {
                if self.current_has_options() {
                    self.move_selection(false);
                }
            }
            KeyCode::Backspace => {
                if !self.current_has_options() {
                    self.pop_freeform_char();
                }
            }
            KeyCode::Char(ch) => {
                if !self.current_has_options()
                    && !key_event
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                {
                    self.push_freeform_char(ch);
                }
            }
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.complete
    }

    fn on_ctrl_c(&mut self, _pane: &mut BottomPane<'_>) -> CancellationEvent {
        if self.submitting {
            return CancellationEvent::Handled;
        }
        self.complete = true;
        CancellationEvent::Handled
    }

    fn handle_paste(&mut self, text: String) -> super::bottom_pane_view::ConditionalUpdate {
        if self.current_has_options() {
            return super::bottom_pane_view::ConditionalUpdate::NoRedraw;
        }
        if text.is_empty() {
            return super::bottom_pane_view::ConditionalUpdate::NoRedraw;
        }
        if let Some(answer) = self.current_answer_mut() {
            answer.freeform.push_str(&text);
        }
        super::bottom_pane_view::ConditionalUpdate::NeedsRedraw
    }

    fn desired_height(&self, width: u16) -> u16 {
        let inner_width = width.saturating_sub(2);
        let question = self.current_question();

        let prompt_lines = question
            .map(|q| {
                let wrapped = textwrap::wrap(&q.question, inner_width.max(1) as usize);
                u16::try_from(wrapped.len()).unwrap_or(u16::MAX)
            })
            .unwrap_or(1)
            .clamp(1, 3);

        let options_len = question
            .and_then(|q| q.options.as_ref())
            .map(std::vec::Vec::len)
            .unwrap_or(0);

        let options_lines = if options_len > 0 {
            u16::try_from(options_len.min(6)).unwrap_or(6).max(2)
        } else {
            3
        };

        // Borders (2) + progress (1) + header (1) + prompt (N) + content (M) + footer (1)
        // + 1 to account for BottomPane's reserved bottom padding line.
        (2 + 1 + 1 + prompt_lines + options_lines + 1 + 1).max(8)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title("User input")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let question_count = self.question_count();
        let (header, prompt, options) = self
            .current_question()
            .map(|q| (
                q.header.as_str(),
                q.question.as_str(),
                q.options.as_ref(),
            ))
            .unwrap_or(("No questions", "", None));

        let mut y = inner.y;
        let progress = if question_count > 0 {
            format!("Question {}/{}", self.current_idx + 1, question_count)
        } else {
            "Question 0/0".to_string()
        };
        Paragraph::new(Line::from(progress).dim()).render(
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            },
            buf,
        );
        y = y.saturating_add(1);

        Paragraph::new(Line::from(header.bold())).render(
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            },
            buf,
        );
        y = y.saturating_add(1);

        let footer_height = 1u16;
        let available = inner
            .height
            .saturating_sub((y - inner.y).saturating_add(footer_height));
        let has_options = options.is_some_and(|opts| !opts.is_empty());
        let desired_prompt_height = if prompt.trim().is_empty() {
            0u16
        } else {
            let wrapped = textwrap::wrap(prompt, inner.width.max(1) as usize);
            u16::try_from(wrapped.len()).unwrap_or(u16::MAX).clamp(1, 3)
        };

        // Always reserve at least 2 rows so option pickers are usable.
        let (prompt_height, content_height) = if available == 0 {
            (0, 0)
        } else if available <= 2 {
            (0, available)
        } else {
            let prompt_budget = available.saturating_sub(2);
            let prompt_height = desired_prompt_height.min(prompt_budget);
            let content_height = available.saturating_sub(prompt_height);
            (prompt_height, content_height)
        };

        if prompt_height > 0 {
            let prompt_rect = Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: prompt_height,
            };
            Paragraph::new(prompt)
                .wrap(Wrap { trim: true })
                .render(prompt_rect, buf);
            y = y.saturating_add(prompt_height);
        }

        let content_rect = Rect {
            x: inner.x,
            y,
            width: inner.width,
            height: content_height,
        };

        if content_rect.height > 0 {
            if let Some(options) = options.filter(|opts| !opts.is_empty()) {
                let state = self
                    .current_answer()
                    .map(|answer| answer.option_state)
                    .unwrap_or_default();
                let selected = state.selected_idx;
                let rows = options
                    .iter()
                    .enumerate()
                    .map(|(idx, opt)| {
                        let prefix = if selected.is_some_and(|sel| sel == idx) {
                            "(x)"
                        } else {
                            "( )"
                        };
                        GenericDisplayRow {
                            name: format!("{prefix} {}", opt.label),
                            description: Some(opt.description.clone()),
                            match_indices: None,
                            is_current: false,
                            name_color: None,
                        }
                    })
                    .collect::<Vec<_>>();
                render_rows(content_rect, buf, &rows, &state, rows.len().max(1), false);
            } else {
                let text = self
                    .current_answer()
                    .map(|a| a.freeform.as_str())
                    .unwrap_or("");
                let placeholder = "Type your answer…";
                let display = if text.is_empty() {
                    Line::from(placeholder).dim()
                } else {
                    Line::from(text.to_string())
                };
                Paragraph::new(display)
                    .wrap(Wrap { trim: true })
                    .render(content_rect, buf);
            }
        }

        let footer_y = inner.y.saturating_add(inner.height).saturating_sub(1);
        let is_last = question_count > 0 && self.current_idx + 1 >= question_count;
        let enter_label = if is_last { "submit" } else { "next" };
        let footer = if has_options {
            format!(
                "↑/↓ select | Enter {enter_label} | Esc type in composer | PgUp/PgDn prev/next"
            )
        } else {
            format!(
                "Type answer | Enter {enter_label} | Esc type in composer | PgUp/PgDn prev/next"
            )
        };
        Paragraph::new(Line::from(vec![Span::raw(footer)]).dim()).render(
            Rect {
                x: inner.x,
                y: footer_y,
                width: inner.width,
                height: 1,
            },
            buf,
        );
    }
}
