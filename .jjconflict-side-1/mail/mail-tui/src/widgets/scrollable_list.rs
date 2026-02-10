#![allow(unused, clippy::module_name_repetitions)]
use crossterm::event::{Event, KeyCode};
use ratatui::prelude::Constraint::Length;
use ratatui::prelude::*;
use ratatui::symbols::scrollbar;
use ratatui::widgets::ScrollbarOrientation::VerticalRight;
use ratatui::widgets::{List, ListState, Scrollbar, ScrollbarState};

use crate::widgets::utils::ScrollableState;

impl ScrollableState for ScrollableListState {
    fn next(&mut self) {
        if self.element_len == 0 {
            return;
        }
        if let Some(index) = self.list_state.selected() {
            let next_index = (self.element_len - 1).min(index + 1);
            self.list_state.select(Some(next_index));
        }
    }

    fn prev(&mut self) {
        if self.element_len == 0 {
            return;
        }
        if let Some(index) = self.list_state.selected() {
            let next_index = index.saturating_sub(1);
            self.list_state.select(Some(next_index));
        }
    }
}

pub struct ScrollableListState {
    list_state: ListState,
    scroll_state: ScrollbarState,
    element_len: usize,
    focused: bool,
}

impl ScrollableListState {
    pub fn new(selected: Option<usize>) -> Self {
        Self {
            list_state: ListState::default().with_selected(selected),
            scroll_state: ScrollbarState::default(),
            element_len: 0,
            focused: true,
        }
    }

    pub fn selected(&self) -> Option<usize> {
        self.list_state.selected()
    }

    pub fn select(&mut self, index: Option<usize>) {
        self.list_state.select(index);
    }

    pub fn set_offset(&mut self, offset: usize) {
        *self.list_state.offset_mut() = offset;
    }

    pub fn set_len(&mut self, len: usize) {
        self.element_len = len;
    }

    pub fn len(&mut self) -> usize {
        self.element_len
    }

    pub fn set_focus_gained(&mut self) {
        self.focused = true;
    }
    pub fn set_focus_lost(&mut self) {
        self.focused = false;
    }

    pub fn focused(mut self, value: bool) -> Self {
        self.focused = value;
        self
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }
}

pub struct ScrollableList<'a>(List<'a>);

impl<'a> ScrollableList<'a> {
    pub fn new(list: List<'a>) -> Self {
        Self(list)
    }
}

impl StatefulWidget for ScrollableList<'_> {
    type State = ScrollableListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let [main_area, scroll_area] =
            Layout::horizontal([Constraint::Fill(10), Length(1)]).areas(area);

        state.element_len = self.0.len();
        let total_height = state.element_len;
        let visible_area = main_area.height as usize;

        let draw_scroll_bar = if total_height >= visible_area {
            let content_length = total_height - visible_area;
            state.scroll_state = state
                .scroll_state
                .content_length(content_length)
                .viewport_content_length(1)
                .position(state.list_state.offset());
            true
        } else {
            false
        };

        let main_area = if draw_scroll_bar { main_area } else { area };
        let mut list = self.0;
        if state.focused {
            list = list.highlight_style(Style::default().reversed());
        }
        StatefulWidget::render(list, main_area, buf, &mut state.list_state);
        if draw_scroll_bar {
            Scrollbar::new(VerticalRight)
                .symbols(scrollbar::VERTICAL)
                .render(scroll_area, buf, &mut state.scroll_state);
        }
    }
}
