//! Quality gate modal for escalated questions
//!
//! Displays questions from quality gates that need human input,
//! batched by checkpoint with progress indicators and context.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use std::collections::HashMap;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::chatwidget::spec_kit::{
    EscalatedQuestion, Magnitude, QualityCheckpoint,
};

use super::{BottomPane, BottomPaneView, CancellationEvent};

/// Modal state for quality gate questions
pub(crate) struct QualityGateModal {
    checkpoint: QualityCheckpoint,
    questions: Vec<EscalatedQuestion>,
    current_index: usize,
    answers: HashMap<String, String>,
    current_input: String,
    app_event_tx: AppEventSender,
    done: bool,
}

impl QualityGateModal {
    pub fn new(
        checkpoint: QualityCheckpoint,
        questions: Vec<EscalatedQuestion>,
        app_event_tx: AppEventSender,
    ) -> Self {
        Self {
            checkpoint,
            questions,
            current_index: 0,
            answers: HashMap::new(),
            current_input: String::new(),
            app_event_tx,
            done: false,
        }
    }

    fn current_question(&self) -> Option<&EscalatedQuestion> {
        self.questions.get(self.current_index)
    }

    fn total_questions(&self) -> usize {
        self.questions.len()
    }

    fn is_last_question(&self) -> bool {
        self.current_index >= self.questions.len().saturating_sub(1)
    }

    fn submit_current_answer(&mut self) {
        if let Some(question) = self.current_question() {
            if !self.current_input.trim().is_empty() {
                self.answers.insert(question.id.clone(), self.current_input.trim().to_string());

                if self.is_last_question() {
                    // All questions answered - send completion event
                    self.send_completion_event();
                    self.done = true;
                } else {
                    // Move to next question
                    self.current_index += 1;
                    self.current_input.clear();
                }
            }
        }
    }

    fn send_completion_event(&self) {
        let _ = self.app_event_tx.send(AppEvent::QualityGateAnswersSubmitted {
            checkpoint: self.checkpoint,
            answers: self.answers.clone(),
        });
    }

    pub fn is_complete(&self) -> bool {
        self.done
    }

    pub fn desired_height(&self, _width: u16) -> u16 {
        // Rough estimate: title + question + context + agent answers + input + footer
        // Will be more precise based on actual content
        let base_height = 20;

        // Add space for agent answers
        let agent_answer_lines = self.current_question()
            .map(|q| q.agent_answers.len())
            .unwrap_or(0);

        // Add space for GPT-5 reasoning if present
        let gpt5_lines = self.current_question()
            .and_then(|q| q.gpt5_reasoning.as_ref())
            .map(|_| 3)
            .unwrap_or(0);

        base_height + agent_answer_lines as u16 + gpt5_lines
    }

    fn handle_text_input(&mut self, c: char) {
        if !self.done {
            self.current_input.push(c);
        }
    }

    fn handle_backspace(&mut self) {
        if !self.done {
            self.current_input.pop();
        }
    }

    fn handle_submit(&mut self) {
        if !self.done {
            self.submit_current_answer();
        }
    }

    fn handle_escape(&mut self) {
        // Cancel entire quality gate
        self.done = true;
        let _ = self.app_event_tx.send(AppEvent::QualityGateCancelled {
            checkpoint: self.checkpoint,
        });
    }

    fn magnitude_badge(&self, magnitude: Magnitude) -> Span<'static> {
        match magnitude {
            Magnitude::Critical => Span::styled(
                " CRITICAL ",
                Style::default().fg(Color::White).bg(Color::Red).bold(),
            ),
            Magnitude::Important => Span::styled(
                " IMPORTANT ",
                Style::default().fg(Color::Black).bg(Color::Yellow).bold(),
            ),
            Magnitude::Minor => Span::styled(
                " MINOR ",
                Style::default().fg(Color::White).bg(Color::Blue),
            ),
        }
    }
}

impl BottomPaneView<'_> for QualityGateModal {
    fn handle_key_event(&mut self, _pane: &mut BottomPane, key_event: KeyEvent) {
        if self.done {
            return;
        }

        if matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            match key_event.code {
                KeyCode::Char(c) => self.handle_text_input(c),
                KeyCode::Backspace => self.handle_backspace(),
                KeyCode::Enter => self.handle_submit(),
                KeyCode::Esc => self.handle_escape(),
                _ => {}
            }
        }
    }

    fn on_ctrl_c(&mut self, _pane: &mut BottomPane) -> CancellationEvent {
        self.handle_escape();
        CancellationEvent::Handled
    }

    fn is_complete(&self) -> bool {
        self.done
    }

    fn desired_height(&self, width: u16) -> u16 {
        QualityGateModal::desired_height(self, width)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        // Main border
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(Color::Cyan))
            .title(format!(
                " Quality Gate: {} ",
                match self.checkpoint {
                    QualityCheckpoint::PrePlanning => "Pre-Planning",
                    QualityCheckpoint::PostPlan => "Post-Plan",
                    QualityCheckpoint::PostTasks => "Post-Tasks",
                }
            ));

        let inner = block.inner(area);
        block.render(area, buf);

        let Some(question) = self.current_question() else {
            // No questions - shouldn't happen
            return;
        };

        // Split inner area into sections
        let mut lines = Vec::new();

        // Progress indicator
        lines.push(Line::from(vec![
            Span::styled(
                format!("Question {} of {} ", self.current_index + 1, self.total_questions()),
                Style::default().bold(),
            ),
            self.magnitude_badge(question.magnitude),
        ]));
        lines.push(Line::from(""));

        // Question text
        lines.push(Line::from(vec![
            Span::styled("Q: ", Style::default().fg(Color::Yellow).bold()),
            Span::raw(&question.question),
        ]));
        lines.push(Line::from(""));

        // Context
        if !question.context.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Context: ", Style::default().dim()),
                Span::raw(&question.context),
            ]));
            lines.push(Line::from(""));
        }

        // Agent answers
        lines.push(Line::from(Span::styled(
            "Agent Answers:",
            Style::default().underlined(),
        )));

        for (agent, answer) in &question.agent_answers {
            lines.push(Line::from(vec![
                Span::styled(format!("  {}: ", agent), Style::default().fg(Color::Green)),
                Span::raw(answer),
            ]));
        }
        lines.push(Line::from(""));

        // GPT-5 reasoning if present
        if let Some(gpt5_reasoning) = &question.gpt5_reasoning {
            lines.push(Line::from(vec![
                Span::styled("⚠ GPT-5: ", Style::default().fg(Color::Red).bold()),
                Span::raw("Rejected majority answer"),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  Reason: ", Style::default().dim()),
                Span::raw(gpt5_reasoning),
            ]));
            lines.push(Line::from(""));
        }

        // Suggested options if available
        if !question.suggested_options.is_empty() {
            lines.push(Line::from(Span::styled(
                "Suggested Options:",
                Style::default().dim(),
            )));
            for (idx, option) in question.suggested_options.iter().enumerate() {
                lines.push(Line::from(vec![
                    Span::styled(format!("  [{}] ", idx + 1), Style::default().fg(Color::Cyan)),
                    Span::raw(option),
                ]));
            }
            lines.push(Line::from(""));
        }

        // Input prompt
        lines.push(Line::from(vec![
            Span::styled("Your answer: ", Style::default().fg(Color::Cyan).bold()),
            Span::styled(&self.current_input, Style::default().fg(Color::White)),
            Span::styled("█", Style::default().fg(Color::Gray)), // Cursor
        ]));
        lines.push(Line::from(""));

        // Footer hints
        lines.push(Line::from(vec![
            Span::styled("[Enter]", Style::default().fg(Color::Green)),
            Span::raw(" Submit  "),
            Span::styled("[Esc]", Style::default().fg(Color::Red)),
            Span::raw(" Cancel all"),
        ]));

        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((0, 0));

        paragraph.render(inner, buf);
    }
}

// TODO: Add these event variants to AppEvent enum in app_event.rs:
// - QualityGateAnswersSubmitted { checkpoint: QualityCheckpoint, answers: HashMap<String, String> }
// - QualityGateCancelled { checkpoint: QualityCheckpoint }
