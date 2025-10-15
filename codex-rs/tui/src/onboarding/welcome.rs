use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Widget;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::WidgetRef;

use crate::onboarding::onboarding_screen::StepStateProvider;

use super::onboarding_screen::StepState;

pub(crate) struct WelcomeWidget {
    pub is_logged_in: bool,
}

impl WidgetRef for &WelcomeWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let line1 = Line::from(vec![
            Span::raw(">_ "),
            Span::styled(
                "Welcome to Code",
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]);
        line1.render(area, buf);

        // Render second line below the first
        if area.height > 1 {
            let line2 = Line::from(vec![
                Span::raw("   "), // Indent to align with text after ">_ "
                Span::raw(crate::greeting::greeting_placeholder()),
            ]);
            let line2_area = Rect {
                x: area.x,
                y: area.y + 1,
                width: area.width,
                height: 1,
            };
            line2.render(line2_area, buf);
        }
    }
}

impl StepStateProvider for WelcomeWidget {
    fn get_step_state(&self) -> StepState {
        match self.is_logged_in {
            true => StepState::Hidden,
            false => StepState::Complete,
        }
    }
}
