#![allow(unused, clippy::module_name_repetitions)]
use ratatui::prelude::Constraint::Length;
use ratatui::prelude::*;
use ratatui::symbols::scrollbar;
use ratatui::widgets::ScrollbarOrientation::VerticalRight;
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarState};

pub struct ScrollableParagraphState {
    scroll_state: ScrollbarState,
    offset: usize,
}

impl ScrollableParagraphState {
    pub fn new() -> Self {
        Self {
            scroll_state: ScrollbarState::default(),
            offset: 0,
        }
    }

    pub fn scroll_up(&mut self) {
        self.offset = self.offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.offset = (self.offset + 1).min(u16::MAX as usize);
    }
}

pub struct ScrollableParagraph<'a> {
    widget: Paragraph<'a>,
    content_length: usize,
}

impl<'a> ScrollableParagraph<'a> {
    pub fn new(widget: Paragraph<'a>, len: usize) -> Self {
        Self {
            widget,
            content_length: len,
        }
    }
}

impl<'a> StatefulWidget for ScrollableParagraph<'a> {
    type State = ScrollableParagraphState;

    fn render(mut self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let [main_area, scroll_area] =
            Layout::horizontal([Constraint::Fill(10), Length(1)]).areas(area);

        let total_height = self.content_length;
        let visible_area = main_area.height as usize;

        let draw_scroll_bar = if total_height >= visible_area {
            let content_length = total_height - visible_area;
            state.offset = state.offset.min(content_length);
            self.widget = self
                .widget
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
        Widget::render(self.widget, main_area, buf);
        if draw_scroll_bar {
            Scrollbar::new(VerticalRight)
                .symbols(scrollbar::VERTICAL)
                .render(scroll_area, buf, &mut state.scroll_state);
        }
    }
}
