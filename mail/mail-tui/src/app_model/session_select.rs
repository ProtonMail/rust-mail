use crate::app::Command;
use crate::app_model::{login, mailbox, AppState, AppStateHandler};
use crate::messages::Messages;
use crate::widgets::{ScrollableList, ScrollableListState};
use anyhow::anyhow;
use crossterm::event::{Event, KeyCode};
use proton_core_common::db::EncryptedUserSession;
use proton_mail_common::exports::tracing;
use proton_mail_common::{MailContext, MailContextError};
use ratatui::layout::Flex;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

pub enum Message {
    Submit,
    NewAccount,
    Init,
    Delete,
}

pub struct Model {
    sessions: Vec<EncryptedUserSession>,
    session_list_state: ScrollableListState,
}

impl Model {
    pub fn new(ctx: &MailContext) -> Result<Self, MailContextError> {
        let sessions = ctx.sessions()?;

        Ok(Self {
            sessions,
            session_list_state: ScrollableListState::new(Some(0)),
        })
    }
}

impl AppStateHandler for Model {
    fn on_state_enter(&mut self) -> Command<Messages> {
        Command::message(Message::Init.into())
    }
    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        let Event::Key(key) = event else {
            return Command::None;
        };

        match key.code {
            KeyCode::Char('n') => Command::message(Message::NewAccount.into()),
            KeyCode::Char('d') => Command::message(Message::Delete.into()),
            KeyCode::Up => {
                self.session_list_state.prev();
                Command::None
            }
            KeyCode::Down => {
                self.session_list_state.next();
                Command::None
            }
            KeyCode::Enter => Command::message(Message::Submit.into()),
            _ => Command::None,
        }
    }

    fn update(&mut self, ctx: &MailContext, message: Messages) -> Command<Messages> {
        let Messages::SessionSelect(message) = message else {
            return Command::None;
        };
        match message {
            Message::Delete => {
                let Some(index) = self.session_list_state.selected() else {
                    return Command::message(Messages::DisplayError(
                        None,
                        anyhow!("No session selected"),
                    ));
                };

                {
                    let Some(session) = self.sessions.get(index) else {
                        return Command::message(Messages::DisplayError(
                            None,
                            anyhow!("Invalid session index",),
                        ));
                    };

                    if let Err(e) = ctx.delete_session(session) {
                        let e = anyhow!("Failed to delete session: {e}");
                        tracing::error!("{e}");
                        return Command::message(Messages::DisplayError(None, e));
                    }
                };

                self.sessions.remove(index);
                Command::None
            }
            Message::Submit => {
                let Some(index) = self.session_list_state.selected() else {
                    return Command::message(Messages::DisplayError(
                        None,
                        anyhow!("No session selected"),
                    ));
                };

                let Some(session) = self.sessions.get(index) else {
                    return Command::message(Messages::DisplayError(
                        None,
                        anyhow!("Invalid session index",),
                    ));
                };

                match ctx.user_context_from_session(session, None) {
                    Ok(context) => Command::message(match mailbox::Model::new(context) {
                        Ok(model) => Messages::SwitchAppState(model.into()),
                        Err(e) => e.into(),
                    }),
                    Err(e) => {
                        let e = anyhow!("Failed to load session: {e}");
                        tracing::error!("{e}");
                        Command::message(Messages::DisplayError(None, e))
                    }
                }
            }
            Message::NewAccount => {
                Command::message(Messages::SwitchAppState(login::Model::new().into()))
            }
            Message::Init => {
                if self.sessions.is_empty() {
                    Command::message(Messages::SwitchAppState(login::Model::new().into()))
                } else {
                    Command::None
                }
            }
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let area = area.inner(Margin {
            horizontal: 10,
            vertical: 2,
        });

        let [_, area, _] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Min(40),
            Constraint::Fill(1),
        ])
        .flex(Flex::Center)
        .areas(area);

        let list_sessions = self
            .sessions
            .iter()
            .map(|session| ListItem::new(Text::from(session.email.clone())))
            .collect::<Vec<_>>();
        self.session_list_state.set_len(self.sessions.len());

        frame.render_stateful_widget(
            ScrollableList::new(
                List::new(list_sessions)
                    .block(Block::new().title("Sessions").borders(Borders::all())),
            ),
            area,
            &mut self.session_list_state,
        );
    }

    fn view_help_bar(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            Line::from(vec![
                Span::from("Enter: ").bold(),
                Span::from("Submit"),
                Span::from(" ▲: ").bold(),
                Span::from("Up"),
                Span::from(" ▼: ").bold(),
                Span::from("Down"),
                Span::from(" N: ").bold(),
                Span::from("New Login"),
                Span::from(" D: ").bold(),
                Span::from("Delete"),
            ]),
            area,
        );
    }

    fn view_status_bar(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(Text::from("Session Select"), area);
    }
}

impl From<Model> for AppState {
    fn from(value: Model) -> Self {
        Self::SessionSelect(value)
    }
}

impl From<Message> for Messages {
    fn from(value: Message) -> Self {
        Self::SessionSelect(value)
    }
}
