use crate::CLI_ARGS;
use crate::app::Command;
use crate::app_model::login::LoginModel;
use crate::app_model::{AppState, AppStateHandler, YesNoPopup, mailbox};
use crate::messages::Messages;
use crate::widgets::utils::ScrollableState;
use crate::widgets::{ScrollableList, ScrollableListState};
use anyhow::{Context as _, anyhow};
use futures::StreamExt as _;
use futures::stream::iter;
use proton_core_common::CoreAccountState;
use proton_core_common::db::account::CoreAccount;
use proton_mail_common::{MailContext, MailContextError, ShouldInitializeMailUserContext};
use ratatui::Frame;
use ratatui::crossterm::event::{Event, KeyCode};
use ratatui::layout::Flex;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};
use std::sync::Arc;
use tracing::{debug, error};

pub enum Message {
    Submit,
    NewAccount,
    Init,
    Delete,
    Logout,
    DeleteSuccess(String),
}

pub struct SessionSelectModel {
    accounts: Vec<(CoreAccount, Option<CoreAccountState>)>,
    session_list_state: ScrollableListState,
}

impl SessionSelectModel {
    pub async fn new(ctx: &MailContext) -> Result<Self, MailContextError> {
        let accounts = ctx.get_accounts().await?;
        let index = accounts
            .iter()
            .position(|ac| ac.username == CLI_ARGS.username)
            .unwrap_or(0);

        let accounts = iter(accounts)
            .then(|account| async {
                let state = ctx
                    .get_account_state(account.remote_id.clone())
                    .await
                    .unwrap_or(None);
                (account, state)
            })
            .collect()
            .await;

        Ok(Self {
            accounts,
            session_list_state: ScrollableListState::new(Some(index)),
        })
    }
}

impl AppStateHandler for SessionSelectModel {
    fn on_state_enter(&mut self) -> Command<Messages> {
        Command::message(Message::Init)
    }
    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        let Event::Key(key) = event else {
            return Command::None;
        };
        if self.session_list_state.handle_event(key.code) {
            return Command::None;
        }

        match key.code {
            KeyCode::Char('n') => Command::message(Message::NewAccount),
            KeyCode::Char('d') => Command::message(Message::Delete),
            KeyCode::Char('l') => Command::message(Message::Logout),
            KeyCode::Enter => Command::message(Message::Submit),
            _ => Command::None,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn update(&mut self, ctx: &Arc<MailContext>, message: Messages) -> Command<Messages> {
        let Messages::SessionSelect(message) = message else {
            return Command::None;
        };
        match message {
            Message::Logout => {
                let Some(index) = self.session_list_state.selected() else {
                    return Command::message(Messages::DisplayError(
                        None,
                        anyhow!("No session selected"),
                    ));
                };

                let Some((account, _stase)) = self.accounts.get(index) else {
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
                        "Confirm Account Logout",
                        format!(
                            "Are you sure you wish to logout '{account_email}' and delete all its data",
                        ),
                    )
                        .on_accept(
                            Command::batch([
                                Command::message(Messages::DisplayBackgroundProgress(format!("Logging out account {account_email}"))),
                            Command::task(async move {
                            let cmd =if let Err(e) = ctx.logout_account_and_delete_user_data(remote_id).await {
                                let e = anyhow!("Failed to delete session: {e}");
                                error!("{e:?}");
                                Command::message(Messages::DisplayError(None, e))
                            } else {
                                Command::none()
                            };
                                Command::batch([
                                    Command::message(Messages::DismissBackgroundProgress),
                                    cmd,
                                ])
                        })]),
                )))
            }
            Message::Delete => {
                let Some(index) = self.session_list_state.selected() else {
                    return Command::message(Messages::DisplayError(
                        None,
                        anyhow!("No session selected"),
                    ));
                };

                let Some((account, _state)) = self.accounts.get(index) else {
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
                    .on_accept(Command::batch([
                        Command::message(Messages::DisplayBackgroundProgress(format!(
                            "Deleting account {account_email}"
                        ))),
                        Command::task(async move {
                            let cmd = if let Err(e) = ctx.delete_account(remote_id).await {
                                let e = anyhow!("Failed to delete session: {e}");
                                error!("{e:?}");
                                Command::message(Messages::DisplayError(None, e))
                            } else {
                                Command::message(Message::DeleteSuccess(account_email))
                            };
                            Command::batch([
                                Command::message(Messages::DismissBackgroundProgress),
                                cmd,
                            ])
                        }),
                    ])),
                ))
            }
            Message::Submit => {
                let Some(index) = self.session_list_state.selected() else {
                    return Command::message(Messages::DisplayError(
                        None,
                        anyhow!("No session selected"),
                    ));
                };

                let Some((account, _state)) = self.accounts.get(index).cloned() else {
                    return Command::message(Messages::DisplayError(
                        None,
                        anyhow!("Invalid session index",),
                    ));
                };

                let ctx = Arc::clone(ctx);
                Command::task(async move {
                    let tri = async move {
                        let sessions = ctx.get_account_sessions(account.remote_id.clone()).await?;
                        let c = match sessions.first() {
                            None => {
                                debug!(
                                    "No sessions found for {}. Logging in...",
                                    account.remote_id
                                );
                                Command::message(Messages::SwitchAppState(
                                    LoginModel::with_email(account.name_or_addr.clone()).into(),
                                ))
                            }
                            Some(sess) => {
                                let context = ctx
                                    .user_context_from_session(
                                        sess,
                                        ShouldInitializeMailUserContext::Yes,
                                    )
                                    .await
                                    .context("Error creating MailUserContext")?;
                                let message = mailbox::MailboxModel::new(context).await?;

                                let tok = &sess.account_id;
                                debug!(
                                    "{} sessions found for {}: {}",
                                    sessions.len(),
                                    account.remote_id,
                                    tok
                                );
                                let message = Messages::SwitchAppState(message.into());
                                Command::message(message)
                            }
                        };
                        Ok::<_, anyhow::Error>(c)
                    }
                    .await;

                    match tri {
                        Ok(c) => c,
                        Err(e) => {
                            error!("{e:?}");
                            Command::message(Messages::DisplayError(None, e))
                        }
                    }
                })
            }
            Message::NewAccount => {
                Command::message(Messages::SwitchAppState(LoginModel::new().into()))
            }
            Message::Init => {
                if self.accounts.is_empty() {
                    Command::message(Messages::SwitchAppState(LoginModel::new().into()))
                } else {
                    Command::None
                }
            }
            Message::DeleteSuccess(email) => {
                self.accounts
                    .retain(|(account, _state)| account.name_or_addr != email);
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
            .map(|(session, state)| {
                ListItem::new(Text::from(format!(
                    "{} ({})",
                    session.name_or_addr,
                    match state {
                        Some(CoreAccountState::NotReady) => "NotReady",
                        Some(CoreAccountState::LoggedIn(_)) => "LoggedIn",
                        Some(CoreAccountState::NeedMbp(_)) => "NeedMbp",
                        Some(CoreAccountState::NeedTfa(_)) => "NeedTfa",
                        Some(CoreAccountState::NeedNewPass(_)) => "NeedNewPass",
                        Some(CoreAccountState::LoggedOut) => "LoggedOut",
                        None => "",
                    }
                )))
            })
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

    fn help_options(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("k, ▲", "Go up"),
            ("j, ▼", "Go down"),
            ("enter", "Log in"),
            ("N", "Log in with a new account"),
            ("D", "Delete an account and all of its info"),
            ("L", "Logout an account and delete all of its info"),
        ]
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

impl From<SessionSelectModel> for AppState {
    fn from(value: SessionSelectModel) -> Self {
        Self::SessionSelect(value)
    }
}

impl From<Message> for Messages {
    fn from(value: Message) -> Self {
        Self::SessionSelect(value)
    }
}
