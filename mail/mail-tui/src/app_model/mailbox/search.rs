use crate::app::Command;
use crate::app_model::mailbox::Message;
use crate::messages::Messages;
use crate::widgets::{TextInput, TextInputState};
use crossterm::event::KeyCode;
use proton_mail_common::{MailUserContext, Mailbox};
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::widgets::Clear;
use std::sync::Arc;

use super::messages::MessagesState;

pub struct SearchStatusBar {
    pub search_phrase: String,
    pub total: u64,
}

pub struct Search {
    search: TextInputState,
}

impl Search {
    pub fn new() -> Self {
        Self {
            search: TextInputState::new(),
        }
    }
}

impl Search {
    pub fn view(&mut self, frame: &mut Frame, area: Rect) {
        let areas = Layout::vertical([Constraint::Length(3)]).split(area);

        frame.render_widget(Clear {}, areas[0]);
        frame.render_stateful_widget(TextInput::new("Search:"), areas[0], &mut self.search);
    }

    pub fn handle_event(&mut self, event: &Event) -> Command<Messages> {
        if let Event::Key(key) = &event {
            match key.code {
                KeyCode::Esc => return Command::message(Message::CloseSearchPopup),

                KeyCode::Enter => {
                    return Command::message(Message::SearchSubmit(
                        self.search.value().trim().to_string(),
                    ));
                }

                _ => {
                    self.search.handle_event(event);
                }
            }
        }

        Command::none()
    }

    pub fn update(
        user_ctx: &Arc<MailUserContext>,
        message: Message,
        mbox: &Mailbox,
    ) -> Command<Messages> {
        match message {
            Message::SearchSubmit(search_phrase) => Command::batch(vec![
                Command::message(Message::CloseSearchPopup),
                MessagesState::from_search(user_ctx.to_owned(), mbox.to_owned(), search_phrase),
            ]),
            _ => Command::none(),
        }
    }

    pub fn help_options(vec: &mut Vec<(&'static str, &'static str)>) {
        vec.extend_from_slice(&[("esc", "Close search"), ("enter", "Submit search")]);
    }
}
