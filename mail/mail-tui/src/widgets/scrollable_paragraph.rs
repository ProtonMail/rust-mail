#![allow(unused, clippy::module_name_repetitions)]
use ratatui::prelude::Constraint::Length;
use ratatui::prelude::*;
use ratatui::symbols::scrollbar;
use ratatui::widgets::ScrollbarOrientation::VerticalRight;
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarState};

use crate::widgets::utils::ScrollableState;

#[derive(Default)]
pub struct ScrollableParagraphState {
    scroll_state: ScrollbarState,
    offset: usize,
}

impl ScrollableState for ScrollableParagraphState {
    fn prev(&mut self) {
        self.offset = self.offset.saturating_sub(1);
    }

    fn next(&mut self) {
        self.offset = (self.offset + 1).min(u16::MAX as usize);
    }
}

#[derive(Clone)]
pub struct ScrollableParagraph<'a>(pub Paragraph<'a>);

impl StatefulWidget for ScrollableParagraph<'_> {
    type State = ScrollableParagraphState;

    fn render(mut self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let [main_area, scroll_area] =
            Layout::horizontal([Constraint::Fill(10), Length(1)]).areas(area);

        let total_height = self.0.line_count(main_area.width);
        let visible_area = main_area.height as usize;

        let draw_scroll_bar = if total_height >= visible_area {
            let content_length = total_height - visible_area;
            state.offset = state.offset.min(content_length);
            self.0 = self
                .0
                .scroll((u16::try_from(state.offset).unwrap_or_default(), 0));
            state.scroll_state = state
                .scroll_state
                .content_length(content_length)
                .viewport_content_length(visible_area)
                .position(state.offset);
            true
        } else {
            false
        };

        let main_area = if draw_scroll_bar { main_area } else { area };
        Widget::render(self.0, main_area, buf);
        if draw_scroll_bar {
            Scrollbar::new(VerticalRight)
                .symbols(scrollbar::VERTICAL)
                .render(scroll_area, buf, &mut state.scroll_state);
        }
    }
}
