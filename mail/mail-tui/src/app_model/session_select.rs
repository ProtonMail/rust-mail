use crate::app::Command;
use crate::app_model::{login, mailbox, AppState, AppStateHandler, YesNoPopup};
use crate::messages::Messages;
use crate::widgets::{ScrollableList, ScrollableListState};
use anyhow::anyhow;
use proton_core_common::db::account::CoreAccount;
use proton_mail_common::{MailContext, MailContextError};
use ratatui::crossterm::event::{Event, KeyCode};
use ratatui::layout::Flex;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;
use std::sync::Arc;

pub enum Message {
    Submit,
    NewAccount,
    Init,
    Delete,
    DeleteSuccess(String),
}

pub struct Model {
    accounts: Vec<CoreAccount>,
    session_list_state: ScrollableListState,
}

impl Model {
    pub async fn new(ctx: &MailContext) -> Result<Self, MailContextError> {
        let accounts = ctx.get_accounts().await?;

        Ok(Self {
            accounts,
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

    fn update(&mut self, ctx: &Arc<MailContext>, message: Messages) -> Command<Messages> {
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

                let Some(account) = self.accounts.get(index) else {
                    return Command::message(Messages::DisplayError(
                        None,
                        anyhow!("Invalid session index",),
                    ));
                };

                let account_email = account.name_or_addr.clone();
                let remote_id = account.remote_id.clone();
                let ctx = Arc::clone(ctx);
                Command::message(Messages::raise_popup(
                    YesNoPopup::new(
                        "Confirm Account Delete",
                        format!(
                            "Are you sure you wish to delete '{account_email}' and all its data",
                        ),
                    )
                    .on_accept(Command::task(async move {
                        if let Err(e) = ctx.delete_account(remote_id).await {
                            let e = anyhow!("Failed to delete session: {e}");
                            tracing::error!("{e}");
                            return Command::message(Messages::DisplayError(None, e));
                        }

                        Command::message(Message::DeleteSuccess(account_email).into())
                    })),
                ))
            }
            Message::Submit => {
                let Some(index) = self.session_list_state.selected() else {
                    return Command::message(Messages::DisplayError(
                        None,
                        anyhow!("No session selected"),
                    ));
                };

                let Some(account) = self.accounts.get(index).cloned() else {
                    return Command::message(Messages::DisplayError(
                        None,
                        anyhow!("Invalid session index",),
                    ));
                };

                let ctx = Arc::clone(ctx);
                Command::task(async move {
                    match ctx.get_sessions(account.remote_id.clone()).await {
                        Ok(sessions) => {
                            if sessions.is_empty() {
                                Command::message(Messages::SwitchAppState(
                                    login::Model::with_email(account.name_or_addr.clone()).into(),
                                ))
                            } else {
                                match ctx.user_context_from_session(&sessions[0]).await {
                                    Ok(context) => {
                                        Command::message(match mailbox::Model::new(context).await {
                                            Ok(model) => Messages::SwitchAppState(model.into()),
                                            Err(e) => e.into(),
                                        })
                                    }
                                    Err(e) => {
                                        let e = anyhow!("Failed to load session: {e}");
                                        tracing::error!("{e}");
                                        Command::message(Messages::DisplayError(None, e))
                                    }
                                }
                            }
                        }
                        Err(e) => Command::message(e.into()),
                    }
                })
            }
            Message::NewAccount => {
                Command::message(Messages::SwitchAppState(login::Model::new().into()))
            }
            Message::Init => {
                if self.accounts.is_empty() {
                    Command::message(Messages::SwitchAppState(login::Model::new().into()))
                } else {
                    Command::None
                }
            }
            Message::DeleteSuccess(email) => {
                self.accounts
                    .retain(|account| account.name_or_addr != email);
                Command::none()
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
            .accounts
            .iter()
            .map(|session| ListItem::new(Text::from(session.name_or_addr.clone())))
            .collect::<Vec<_>>();
        self.session_list_state.set_len(self.accounts.len());

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
