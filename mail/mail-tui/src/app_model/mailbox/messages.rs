#![allow(clippy::module_name_repetitions)]
use crate::app_model::mailbox::{new_live_query, Message, ITEM_LIMIT};
use crate::messages::Messages;
use crate::widgets::{AsTable, ScrollableTable, ScrollableTableState};
use crossterm::event::{Event, KeyCode};
use proton_api_mail::exports::proton_sqlite3::Live;
use proton_mail_common::db::{LocalLabel, MessageQuery};
use proton_mail_common::{Mailbox, MailboxError, MailboxResult};
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::Frame;

pub struct MessagesState {
    label: LocalLabel,
    query: Live<MessageQuery>,
    table_state: ScrollableTableState,
}

impl MessagesState {
    pub fn new(mbox: &Mailbox) -> MailboxResult<Self> {
        let Some(label) = mbox
            .user_context()
            .db_read(|conn| conn.label_with_id(mbox.label_id()))?
        else {
            return Err(MailboxError::LabelNotFound(mbox.label_id()));
        };

        Ok(Self {
            label,
            query: mbox.new_messages_query(new_live_query, ITEM_LIMIT)?,
            table_state: ScrollableTableState::new(Some(0)),
        })
    }
    pub fn label(&self) -> &LocalLabel {
        &self.label
    }

    pub fn on_event(&mut self, event: &Event) -> Option<Messages> {
        let Event::Key(key) = event else {
            return None;
        };

        match key.code {
            KeyCode::Up => {
                self.table_state.prev();
                None
            }
            KeyCode::Down => {
                self.table_state.next();
                None
            }
            KeyCode::Char('s') => Some(Message::OpenLabelSelectPopup.into()),
            _ => None,
        }
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let values = self.query.value();
        let Ok(values) = &*values else {
            return;
        };

        self.table_state.set_len(values.len());
        let table = values.as_table();

        let scrollable_table = ScrollableTable::new(table);

        frame.render_stateful_widget(scrollable_table, area, &mut self.table_state);
    }
    pub fn draw_status_bar(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(Text::from(self.label.name.as_str()), area);
    }
}
