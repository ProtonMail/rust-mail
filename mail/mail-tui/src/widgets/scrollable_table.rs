#![allow(unused, clippy::module_name_repetitions)]
use std::collections::HashSet;
use std::mem;

use crossterm::event::KeyEvent;
use ratatui::prelude::Constraint::Length;
use ratatui::prelude::*;
use ratatui::style::Styled;
use ratatui::symbols::scrollbar;
use ratatui::widgets::ScrollbarOrientation::VerticalRight;
use ratatui::widgets::{List, ListState, Scrollbar, ScrollbarState, Table, TableState};

use crate::widgets::IntoTable;

pub struct ScrollableTableState {
    table_state: TableState,
    scroll_state: ScrollbarState,
    marked: HashSet<usize>,
}

impl ScrollableTableState {
    pub fn new(selected: Option<usize>) -> Self {
        Self {
            table_state: TableState::default().with_selected(selected),
            scroll_state: ScrollbarState::default(),
            marked: Default::default(),
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

    pub fn toggle(&mut self) {
        if let Some(idx) = self.selected() {
            if !self.marked.insert(idx) {
                self.marked.remove(&idx);
            }
        }
    }

    pub fn mark_many(&mut self, indices: impl IntoIterator<Item = usize>) {
        for idx in indices {
            self.marked.insert(idx);
        }
    }

    pub fn unmark_many(&mut self, indices: impl IntoIterator<Item = usize>) {
        for idx in indices {
            self.marked.remove(&idx);
        }
    }

    pub fn help_options(vec: &mut Vec<(&'static str, &'static str)>) {
        vec.extend_from_slice(&[
            ("esc", "Exit composer"),
            ("tab", "Toggle between fields"),
            ("Ctrl + s", "Save"),
            ("Ctrl + t", "Send"),
            ("Ctrl + a", "Add attachment"),
            ("Ctrl + d", "Remove attachment"),
        ]);
    }
}

pub struct ScrollableTable<'a> {
    widget: IntoTable<'a>,
    num_rows: usize,
}

impl<'a> ScrollableTable<'a> {
    pub fn new(table: IntoTable<'a>, num_rows: usize) -> Self {
        Self {
            widget: table,
            num_rows,
        }
    }
}

impl StatefulWidget for ScrollableTable<'_> {
    type State = ScrollableTableState;

    fn render(mut self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
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

        for (idx, row) in self.widget.rows.iter_mut().enumerate() {
            if state.marked.contains(&idx) {
                let row_2 = mem::take(row);
                *row = row_2.style(Style::new().bg(Color::LightBlue));
            } else {
                let row_2 = mem::take(row);
                *row = row_2.style(Style::new().bg(Color::Black));
            }
        }

        let table = self.widget.into_table();

        StatefulWidget::render(table, main_area, buf, &mut state.table_state);

        if draw_scroll_bar {
            Scrollbar::new(VerticalRight)
                .symbols(scrollbar::VERTICAL)
                .render(scroll_area, buf, &mut state.scroll_state);
        }
    }
}
