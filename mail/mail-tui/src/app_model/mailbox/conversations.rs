#![allow(clippy::module_name_repetitions)]
use crate::app_model::mailbox::{new_live_query, Item, Message, ITEM_LIMIT};
use crate::messages::Messages;
use crate::widgets::{AsTable, ScrollableTable, ScrollableTableState};
use crossterm::event::{Event, KeyCode};
use proton_api_mail::exports::proton_sqlite3::Live;
use proton_api_mail::exports::tracing;
use proton_mail_common::db::{ConversationQuery, LocalConversationId, LocalLabel};
use proton_mail_common::{Mailbox, MailboxError, MailboxResult};
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::Frame;

pub struct ConversationsState {
    label: LocalLabel,
    query: Live<ConversationQuery>,
    table_state: ScrollableTableState,
}

impl ConversationsState {
    pub fn new(mbox: &Mailbox) -> MailboxResult<Self> {
        let Some(label) = mbox
            .user_context()
            .db_read(|conn| conn.label_with_id(mbox.label_id()))?
        else {
            return Err(MailboxError::LabelNotFound(mbox.label_id()));
        };

        Ok(Self {
            label,
            query: mbox.new_conversation_query(new_live_query, ITEM_LIMIT)?,
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
            KeyCode::Char('u') => self
                .selected_conversation()
                .map(|id| Message::MarkConversationUnread(id).into()),
            KeyCode::Char('r') => self
                .selected_conversation()
                .map(|id| Message::MarkConversationRead(id).into()),
            KeyCode::Char('d') => self
                .selected_conversation()
                .map(|id| Message::DeleteConversation(id).into()),
            KeyCode::Char('m') => self
                .selected_conversation()
                .map(|id| Message::OpenMoveItemPopup(Item::Conversation(id)).into()),
            KeyCode::Char('l') => self
                .selected_conversation()
                .map(|id| Message::OpenLabelItemPopup(Item::Conversation(id)).into()),
            KeyCode::Char('L') => self
                .selected_conversation()
                .map(|id| Message::OpenUnlabelItemPopup(Item::Conversation(id)).into()),
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

    fn selected_conversation(&self) -> Option<LocalConversationId> {
        let index = self.table_state.selected()?;

        match &*self.query.value() {
            Ok(list) => list.get(index).map(|c| c.id),
            Err(e) => {
                tracing::error!("Can not select conversation, query failed: {e}");
                None
            }
        }
    }
}
