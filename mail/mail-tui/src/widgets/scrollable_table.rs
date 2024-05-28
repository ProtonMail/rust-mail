#![allow(unused, clippy::module_name_repetitions)]
use ratatui::prelude::Constraint::Length;
use ratatui::prelude::*;
use ratatui::symbols::scrollbar;
use ratatui::widgets::ScrollbarOrientation::VerticalRight;
use ratatui::widgets::{List, ListState, Scrollbar, ScrollbarState, Table, TableState};

pub struct ScrollableTableState {
    table_state: TableState,
    scroll_state: ScrollbarState,
}

impl ScrollableTableState {
    pub fn new(selected: Option<usize>) -> Self {
        Self {
            table_state: TableState::default().with_selected(selected),
            scroll_state: ScrollbarState::default(),
        }
    }

    pub fn selected(&self) -> Option<usize> {
        self.table_state.selected()
    }

    pub fn select(&mut self, index: Option<usize>) {
        self.table_state.select(index);
    }

    pub fn set_offset(&mut self, offset: usize) {
        *self.table_state.offset_mut() = offset;
    }

    pub fn next(&mut self) {
        if let Some(index) = self.table_state.selected() {
            self.table_state.select(Some(index.saturating_add(1)));
        }
    }

    pub fn prev(&mut self) {
        if let Some(index) = self.table_state.selected() {
            self.table_state.select(Some(index.saturating_sub(1)));
        }
    }
}

pub struct ScrollableTable<'a> {
    widget: Table<'a>,
    num_rows: usize,
}

impl<'a> ScrollableTable<'a> {
    pub fn new(table: Table<'a>, num_rows: usize) -> Self {
        Self {
            widget: table,
            num_rows,
        }
    }
}

impl<'a> StatefulWidget for ScrollableTable<'a> {
    type State = ScrollableTableState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let [main_area, scroll_area] =
            Layout::horizontal([Constraint::Fill(10), Length(1)]).areas(area);

        let total_height = self.num_rows;
        let visible_area = main_area.height as usize;

        if let Some(index) = state.selected() {
            state.select(Some(index.min(self.num_rows.saturating_sub(1))));
        }

        let draw_scroll_bar = if total_height >= visible_area {
            let content_length = total_height - visible_area;
            state.scroll_state = state
                .scroll_state
                .content_length(content_length)
                .viewport_content_length(1)
                .position(state.table_state.offset());
            true
        } else {
            false
        };

        let main_area = if draw_scroll_bar { main_area } else { area };
        StatefulWidget::render(self.widget, main_area, buf, &mut state.table_state);
        if draw_scroll_bar {
            Scrollbar::new(VerticalRight)
                .symbols(scrollbar::VERTICAL)
                .render(scroll_area, buf, &mut state.scroll_state);
        }
    }
}
