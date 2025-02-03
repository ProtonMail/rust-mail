use crate::app::Command;
use crate::app_model::mailbox::model::StateHandler;
use crate::app_model::mailbox::Message;
use crate::messages::Messages;
use crate::widgets::{TextInput, TextInputState};
use crossterm::event::KeyCode;
use proton_mail_common::models::MailSettings;
use proton_mail_common::{MailContext, Mailbox};
use ratatui::crossterm::event::Event;
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::widgets::Clear;
use ratatui::Frame;
use std::sync::Arc;

use super::messages::MessagesState;

pub struct SearchStatusBar {
    pub search_phrase: String,
    pub total: u64,
}

/// Search bar
pub struct Search {
    search: TextInputState,
}

impl Search {
    /// Create a new search bar
    pub fn new() -> Self {
        Self {
            search: TextInputState::new(),
        }
    }
}

impl StateHandler for Search {
    #[allow(clippy::too_many_lines)]
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let areas = Layout::vertical([Constraint::Length(3)]).split(area);
        frame.render_widget(Clear {}, areas[0]);
        frame.render_stateful_widget(TextInput::new("Search:"), areas[0], &mut self.search);
    }

    fn handle_event(&mut self, _: &Mailbox, event: Event) -> Command<Messages> {
        if let Event::Key(key) = &event {
            match key.code {
                KeyCode::Esc => return Command::message(Message::CloseSearchPopup.into()),
                KeyCode::Enter => {
                    return Command::message(
                        Message::SearchSubmit(self.search.value().trim().to_string()).into(),
                    )
                }
                _ => self.search.handle_event(&event),
            };
        }

        Command::none()
    }

    fn update(
        &mut self,
        _ctx: &MailContext,
        message: Message,
        mbox: &Mailbox,
        _mail_settings: &Arc<MailSettings>,
    ) -> Command<Messages> {
        match message {
            Message::SearchSubmit(search_phrase) => Command::batch(vec![
                Command::message(Message::CloseSearchPopup.into()),
                MessagesState::from_search(mbox.clone(), search_phrase),
            ]),
            _ => Command::none(),
        }
    }
}
