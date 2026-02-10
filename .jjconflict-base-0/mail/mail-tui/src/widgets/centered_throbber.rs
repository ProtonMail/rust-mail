use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Margin, Rect};
use ratatui::prelude::StatefulWidget;
use throbber_widgets_tui::{Throbber, ThrobberState};

/// Utility wrapper that displays a centered [`Throbber`] widget in a given area.
pub struct CenteredThrobber<'a> {
    widget: Throbber<'a>,
}

impl<'a> CenteredThrobber<'a> {
    pub fn new(widget: Throbber<'a>) -> Self {
        Self { widget }
    }

    pub fn default_with_label(label: impl Into<ratatui::text::Span<'a>>) -> Self {
        Self {
            widget: Throbber::default()
                .label(label)
                .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
                .use_type(throbber_widgets_tui::WhichUse::Spin),
        }
    }
}

impl StatefulWidget for CenteredThrobber<'_> {
    type State = ThrobberState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let [_, content, _] = Layout::vertical([
            Constraint::Percentage(50),
            Constraint::Length(3),
            Constraint::Percentage(50),
        ])
        .flex(Flex::SpaceAround)
        .areas(area);
        let [_, content, _] = Layout::horizontal([
            Constraint::Percentage(30),
            Constraint::Percentage(40),
            Constraint::Percentage(30),
        ])
        .flex(Flex::SpaceAround)
        .areas(content);
        let [_, spinner_area, _] = Layout::horizontal([
            Constraint::Percentage(50),
            Constraint::Min(10),
            Constraint::Percentage(50),
        ])
        .areas(content.inner(Margin {
            horizontal: 1,
            vertical: 1,
        }));
        state.calc_next();
        self.widget.render(spinner_area, buf, state);
    }
}
