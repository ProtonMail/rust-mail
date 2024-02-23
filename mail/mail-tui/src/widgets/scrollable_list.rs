use crate::style::list_highlight_style;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint::Length;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::prelude::StatefulWidget;
use ratatui::symbols::scrollbar;
use ratatui::widgets::ScrollbarOrientation::VerticalRight;
use ratatui::widgets::{HighlightSpacing, List, ListState, Scrollbar, ScrollbarState};

pub struct ScrollableListState {
    list_state: ListState,
    scroll_state: ScrollbarState,
    element_size: u16,
    element_len: usize,
    focused: bool,
}

//TODO: proper widget version?

impl ScrollableListState {
    pub fn new(element_size: u16, selected: Option<usize>) -> Self {
        Self {
            list_state: ListState::default().with_selected(selected),
            scroll_state: ScrollbarState::default(),
            element_size,
            element_len: 0,
            focused: false,
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

    pub fn next(&mut self) {
        if let Some(index) = self.list_state.selected() {
            let next_index = (self.element_len - 1).min(index + 1);
            self.list_state.select(Some(next_index));
        }
    }

    pub fn prev(&mut self) {
        if let Some(index) = self.list_state.selected() {
            let next_index = index.saturating_sub(1);
            self.list_state.select(Some(next_index));
        }
    }

    pub fn set_len(&mut self, len: usize) {
        self.element_len = len;
    }

    #[allow(unused)]
    pub fn len(&mut self) -> usize {
        self.element_len
    }

    pub fn set_focus_gained(&mut self) {
        self.focused = true;
    }
    pub fn set_focus_lost(&mut self) {
        self.focused = false;
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }
}

pub struct ScrollableList<'a>(List<'a>);

impl<'a> ScrollableList<'a> {
    pub fn new(list: impl Into<List<'a>>) -> Self {
        Self(list.into())
    }
}

impl<'a> StatefulWidget for ScrollableList<'a> {
    type State = ScrollableListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let [main_area, scroll_area] =
            Layout::horizontal([Constraint::Fill(10), Length(1)]).areas(area);

        let total_height = state.element_len * state.element_size as usize;
        let visible_area = main_area.height as usize;

        let draw_scroll_bar = if total_height >= visible_area {
            let content_length = (total_height - visible_area) / state.element_size as usize;
            state.scroll_state = state
                .scroll_state
                .content_length(content_length)
                .viewport_content_length(1)
                .position(state.list_state.offset());
            true
        } else {
            false
        };

        let main_area = if !draw_scroll_bar { area } else { main_area };
        let mut list = self
            .0
            .highlight_spacing(HighlightSpacing::Always)
            .highlight_symbol("> ");
        if state.focused {
            list = list.highlight_style(list_highlight_style());
        }
        list.render(main_area, buf, &mut state.list_state);
        if draw_scroll_bar {
            Scrollbar::new(VerticalRight)
                .symbols(scrollbar::VERTICAL)
                .render(scroll_area, buf, &mut state.scroll_state);
        }
    }
}
