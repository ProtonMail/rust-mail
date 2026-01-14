mod address_list;
mod expiration_time;
mod password_protect;
pub mod recipient_list;
mod schedule_send;

use crate::app::Command;
use crate::app_model::YesNoPopup;
use crate::app_model::mailbox::composer::address_list::AddressListPopup;
use crate::app_model::mailbox::composer::expiration_time::ExpirationTimePopup;
use crate::app_model::mailbox::composer::password_protect::PasswordProtectPopup;
use crate::app_model::mailbox::composer::schedule_send::ScheduleSendPopup;
use crate::app_model::mailbox::{ComposerMessage, Message};
use crate::messages::Messages;
use crate::widgets::utils::ScrollableState;
use crate::widgets::{ScrollableList, ScrollableListState, TextInput, TextInputState};
use anyhow::anyhow;
use chrono::{DateTime, Local};
use crossterm::event::{KeyCode, KeyModifiers};
use futures::FutureExt;
use proton_core_common::models::ModelExtension;
use proton_mail_api::proton_core_api::services::proton::AddressId;
use proton_mail_common::datatypes::{Disposition, LocalAttachmentId, LocalMessageId};
use proton_mail_common::draft::attachments::{DraftAttachment, DraftAttachmentState};
use proton_mail_common::draft::observers::DraftAttachmentObserver;
use proton_mail_common::draft::recipients::RecipientList;
use proton_mail_common::draft::{
    AttachmentDispositionSwapError, Draft, DraftActorOptions, DraftEvent, DraftExpirationTime,
    DraftSyncStatus, RecipientGroupId, ReplyMode,
};
use proton_mail_common::models::{Attachment, MessageMimeType, MetadataId};
use proton_mail_common::{MailContextError, MailUserContext, Mailbox};
use proton_mail_html_transformer::Html2TextOptions;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List};
use recipient_list::TuiRecipientList;
use secrecy::{ExposeSecret, SecretString};
use stash::stash::{Stash, StashError, Tether};
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::error;
use tui_textarea::TextArea;

use super::RecipientListMessage;

/// Composer to edit and view drafts.
pub struct Composer {
    draft: Draft,
    text_area: TextArea<'static>,
    selected_input: SelectedInput,
    sender_input_state: TextInputState,
    to_input_state: TextInputState,
    cc_input_state: TextInputState,
    bcc_input_state: TextInputState,
    subject_input_state: TextInputState,
    attachment_list_state: ScrollableListState,
    attachment_infos: Vec<AttachmentInfo>,
    draft_sync_status: Option<DraftSyncStatus>,
    // for table observer.
    _observer_cancellation_token: CancellationToken,
    recipient_view: Option<TuiRecipientList>,
}

impl Composer {
    /// Create a new draft.
    pub fn empty(ctx: Arc<MailUserContext>) -> Command<Messages> {
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Creating new draft...".to_owned(),
            )),
            Command::task(async move {
                Command::batch([
                    Command::message(Messages::DismissBackgroundProgress),
                    match Draft::empty_ex(&ctx, draft_options()).await {
                        Ok(draft) => Composer::create(draft, None, ctx.user_stash().clone()).await,
                        Err(e) => {
                            error!("Failed to create new draft:{e:?}");
                            Command::Message(Messages::DisplayError(None, e.into()))
                        }
                    },
                ])
            }),
        ])
    }

    /// Reply to a message with `message_id`.
    ///
    /// If the message is not a draft an error will be returned.
    pub fn reply(
        context: Arc<MailUserContext>,
        message_id: LocalMessageId,
        reply_mode: ReplyMode,
    ) -> Command<Messages> {
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Creating draft reply...".to_owned(),
            )),
            Command::task(async move {
                Command::batch([
                    Command::message(Messages::DismissBackgroundProgress),
                    match Draft::reply_ex(&context, message_id, reply_mode, false, draft_options())
                        .await
                    {
                        Ok(draft) => {
                            Composer::create(draft, None, context.user_stash().clone()).await
                        }
                        Err(e) => {
                            error!("Failed to open message in composer: {e:?}");
                            Command::batch([
                                Command::message(Message::CloseComposer),
                                Command::message(Messages::DisplayError(None, e.into())),
                            ])
                        }
                    },
                ])
            }),
        ])
    }

    /// Open an existing draft with for `message_id`.
    ///
    /// If the message is not a draft an error will be returned.
    pub fn open(context: Arc<MailUserContext>, message_id: LocalMessageId) -> Command<Messages> {
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Opening draft...".to_owned(),
            )),
            Command::task(async move {
                Command::batch([
                    Command::message(Messages::DismissBackgroundProgress),
                    match Draft::open_ex(&context, message_id, draft_options()).await {
                        Ok((draft, sync_status)) => {
                            Composer::create(draft, Some(sync_status), context.user_stash().clone())
                                .await
                        }
                        Err(e) => {
                            error!("Failed to open message in composer: {e:?}");
                            Command::batch([
                                Command::message(Message::CloseComposer),
                                Command::message(Messages::from(e)),
                            ])
                        }
                    },
                ])
            }),
        ])
    }

    /// Save a draft.
    fn save(&mut self) -> Command<Messages> {
        let draft = self.draft.clone();
        let composer_state = self.to_composer_draft_state();
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Saving draft...".to_owned(),
            )),
            Command::task(async move {
                Command::batch([
                    Command::message(Messages::DismissBackgroundProgress),
                    match composer_state.apply(&draft).await {
                        Ok(()) => Command::none(),
                        Err(e) => {
                            error!("Failed to save draft: {e:?}");
                            Command::message(e)
                        }
                    },
                ])
            }),
        ])
    }

    fn to_composer_draft_state(&self) -> ComposerDraftState {
        ComposerDraftState {
            subject: self.subject_input_state.value().to_owned(),
            body: self.text_area.lines().join("\n"),
        }
    }

    /// Send the draft.
    fn send(&mut self, scheduled_time: Option<DateTime<Local>>) -> Command<Messages> {
        let composer_state = self.to_composer_draft_state();
        let draft = self.draft.clone();
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Sending draft...".to_owned(),
            )),
            Command::task(async move {
                Command::batch([
                    Command::message(Messages::DismissBackgroundProgress),
                    if let Err(e) = composer_state.apply(&draft).await {
                        error!("Failed to save draft: {e:?}");
                        Command::message(e)
                    } else {
                        match if let Some(scheduled_time) = scheduled_time {
                            draft.schedule_send(scheduled_time).await
                        } else {
                            draft.send().await
                        } {
                            Ok(_) => Command::message(Message::CloseComposer),
                            Err(e) => {
                                error!("Failed to send draft: {e:?}");
                                Command::message(e)
                            }
                        }
                    },
                ])
            }),
        ])
    }
    async fn create(
        draft: Draft,
        sync_status: Option<DraftSyncStatus>,
        stash: Stash,
    ) -> Command<Messages> {
        match Self::new_impl(draft, sync_status, stash).await {
            Ok((composer, background_cmd)) => {
                let error_msg = composer
                    .draft
                    .take_address_validation_result()
                    .await
                    .map(|v| {
                        v.map_or(Command::none(), |e| {
                            Command::message(Messages::DisplayError(
                                Some("Address Validation".into()),
                                anyhow!("Address {} is not valid: {}", e.email, e.error),
                            ))
                        })
                    })
                    .unwrap_or(Command::none());
                Command::batch([
                    Command::message(Message::OpenComposer(composer)),
                    error_msg,
                    background_cmd,
                ])
            }
            Err(e) => Command::message(Messages::DisplayError(
                Some("Open Composer failed".to_owned()),
                e.into(),
            )),
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn new_impl(
        draft: Draft,
        sync_status: Option<DraftSyncStatus>,
        stash: Stash,
    ) -> Result<(Self, Command<Messages>), MailContextError> {
        let state = draft.state().await?;
        let to_list = recipient_list_to_display_value(&state.to_list);
        let cc_list = recipient_list_to_display_value(&state.cc_list);
        let bcc_list = recipient_list_to_display_value(&state.bcc_list);

        let text_area = match state.mime_type {
            MessageMimeType::TextHtml => {
                let text = proton_mail_html_transformer::Transformer::html2text_str(
                    &state.body,
                    Html2TextOptions::default(),
                )
                .unwrap_or_else(|e| format!("Failed to parse html:{e}"));

                TextArea::new(text.split('\n').map(str::to_owned).collect())
            }

            MessageMimeType::TextPlain => {
                TextArea::new(state.body.split('\n').map(str::to_owned).collect())
            }
        };

        let tether = stash.connection().await?;
        let attachment_infos = Self::build_attachment_infos(draft.metadata_id, &tether).await?;
        drop(tether);

        let mut observer = DraftAttachmentObserver::new(draft.metadata_id, stash).await?;
        let cancellation_token = CancellationToken::new();
        let cancellation_token_cloned = cancellation_token.clone();

        let mut draft_subscriber = draft.subscribe();
        let dract_subscriber_token = cancellation_token.clone();

        let recipient_background_command = Command::background_task(move |sender| {
            async move {
                dract_subscriber_token
                    .run_until_cancelled_owned(async {
                        while let Ok(event) = draft_subscriber.recv().await {
                            match event {
                                DraftEvent::RecipientListUpdated { group, list } => {
                                    let _ = sender
                                        .send_async(
                                            RecipientListMessage::UpdateRecipients(group, list)
                                                .into(),
                                        )
                                        .await;
                                }
                                DraftEvent::RecipientListsUpdated { to, cc, bcc } => {
                                    let _ = sender
                                        .send_async(
                                            RecipientListMessage::UpdateRecipients(
                                                RecipientGroupId::To,
                                                to,
                                            )
                                            .into(),
                                        )
                                        .await;
                                    let _ = sender
                                        .send_async(
                                            RecipientListMessage::UpdateRecipients(
                                                RecipientGroupId::Cc,
                                                cc,
                                            )
                                            .into(),
                                        )
                                        .await;
                                    let _ = sender
                                        .send_async(
                                            RecipientListMessage::UpdateRecipients(
                                                RecipientGroupId::Bcc,
                                                bcc,
                                            )
                                            .into(),
                                        )
                                        .await;
                                }
                                DraftEvent::Sent | DraftEvent::Discarded => {}
                            }
                        }
                    })
                    .await;
            }
            .boxed()
        });

        let background_cmd = Command::background_task(move |sender| {
            async move {
            loop {
                tokio::select! {
                () = cancellation_token_cloned.cancelled() => {
                    return;
                }
                r = observer.next() =>
                      match r {
                          Ok(()) => {
                              let _ = sender.send_async(Command::message(ComposerMessage::RefreshAttachmentList)).await;
                           }
                          Err(e) => {
                             let _= sender.send_async(Command::message(Messages::DisplayError(Some("Draft Attachment Observer Error".to_owned()),anyhow::Error::new(e)))).await;
                             return;
                          }
                      }
                }
            }
        }.boxed()
        });
        Ok((
            Self {
                draft,
                text_area,
                selected_input: SelectedInput::To,
                sender_input_state: TextInputState::with_value(state.sender),
                to_input_state: TextInputState::with_value(to_list).selected(true),
                cc_input_state: TextInputState::with_value(cc_list),
                bcc_input_state: TextInputState::with_value(bcc_list),
                subject_input_state: TextInputState::with_value(state.subject),
                attachment_list_state: ScrollableListState::new(None),
                attachment_infos,
                draft_sync_status: sync_status,
                _observer_cancellation_token: cancellation_token,
                recipient_view: None,
            },
            Command::batch([background_cmd, recipient_background_command]),
        ))
    }

    /// Collect attachment info to be displayed.
    async fn build_attachment_infos(
        metadata_id: MetadataId,
        tether: &Tether,
    ) -> Result<Vec<AttachmentInfo>, StashError> {
        Ok(DraftAttachment::build_list(metadata_id, tether)
            .await?
            .into_iter()
            .map(AttachmentInfo::from)
            .collect())
    }

    /// Discard the draft.
    fn discard(&mut self) -> Command<Messages> {
        let draft = self.draft.clone();
        let popup = YesNoPopup::new(
            "Discard Draft",
            "Are you sure you wish to discard the current draft?",
        )
        .on_accept(Command::batch([
            Command::message(Message::CloseComposer),
            Command::message(Messages::DisplayBackgroundProgress(
                "Discarding Draft".to_owned(),
            )),
            Command::task(async move {
                let cmd = match draft.discard().await {
                    Ok(_) => Command::none(),
                    Err(e) => Command::message(Messages::DisplayError(None, anyhow::Error::new(e))),
                };
                Command::batch([Command::message(Messages::DismissBackgroundProgress), cmd])
            }),
        ]));

        Command::message(Messages::raise_popup(popup))
    }

    /// Create a new attachment that can be added to the draft.
    fn create_attachment(
        &mut self,
        context: Arc<MailUserContext>,
        path: PathBuf,
    ) -> Command<Messages> {
        let draft = self.draft.clone();
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Preparing Attachment".to_owned(),
            )),
            Command::task(async move {
                let Ok(mut tether) = context.user_stash().connection().await else {
                    return Command::message(Messages::DisplayError(
                        None,
                        anyhow!("Failed acquire db connection"),
                    ));
                };
                let Ok(address_id) = draft.address_id().await else {
                    return Command::batch([
                        Command::message(Messages::DismissBackgroundProgress),
                        Command::message(Messages::DisplayError(
                            None,
                            anyhow!("Failed to get address id"),
                        )),
                    ]);
                };
                let cmd = match Attachment::create_local(
                    &context,
                    address_id,
                    Disposition::Attachment,
                    &path,
                    None,
                    &mut tether,
                )
                .await
                {
                    Ok(attachment) => {
                        Command::message(ComposerMessage::AddAttachment(Box::new(attachment)))
                    }
                    Err(e) => Command::message(e),
                };

                Command::batch([Command::message(Messages::DismissBackgroundProgress), cmd])
            }),
        ])
    }

    /// Add attachment to the draft
    fn add_attachment(&mut self, attachment: Box<Attachment>) -> Command<Messages> {
        let draft = self.draft.clone();
        // Note that we want to make sure the action is queued first
        // before we allow the user to send or we can run into missing depencency issues.
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Adding Attachment to message".to_owned(),
            )),
            Command::task(async move {
                let cmd = if let Err(e) = draft.add_attachment(&attachment).await {
                    e.into()
                } else {
                    Command::message(ComposerMessage::RefreshAttachmentList)
                };

                Command::batch([Command::message(Messages::DismissBackgroundProgress), cmd])
            }),
        ])
    }

    /// Remove an attachment from the draft
    fn remove_attachment(&mut self, id: LocalAttachmentId) -> Command<Messages> {
        let draft = self.draft.clone();
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Removing Attachment from message".to_owned(),
            )),
            Command::task(async move {
                let cmd = if let Err(e) = draft.remove_attachment(id).await {
                    Command::message(e)
                } else {
                    Command::message(ComposerMessage::RefreshAttachmentList)
                };

                Command::batch([Command::message(Messages::DismissBackgroundProgress), cmd])
            }),
        ])
    }

    fn retry_attachment_op(&mut self, id: LocalAttachmentId) -> Command<Messages> {
        let draft = self.draft.clone();
        // Note that we want to make sure the action is queued first
        // before we allow the user to send or we can run into missing depencency issues.
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Retrying attachment op".to_owned(),
            )),
            Command::task(async move {
                let cmd = if let Err(e) = draft.retry_attachment_action(id).await {
                    e.into()
                } else {
                    Command::message(ComposerMessage::RefreshAttachmentList)
                };

                Command::batch([Command::message(Messages::DismissBackgroundProgress), cmd])
            }),
        ])
    }

    fn swap_attachment_disposition(
        &mut self,
        ctx: Arc<MailUserContext>,
        id: LocalAttachmentId,
    ) -> Command<Messages> {
        let draft = self.draft.clone();
        // Note that we want to make sure the action is queued first
        // before we allow the user to send or we can run into missing depencency issues.
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Swapping attachment disposition".to_owned(),
            )),
            Command::task(async move {
                let r = async {
                    let tether = ctx.user_stash().connection().await?;
                    let attachment = Attachment::find_by_id(id, &tether)
                        .await?
                        .ok_or(AttachmentDispositionSwapError::AttachmentNotFound(id))?;

                    let new_disposition = match attachment.disposition {
                        Disposition::Attachment => Disposition::Inline,
                        Disposition::Inline => Disposition::Attachment,
                    };
                    draft.swap_attachment_disposition(id, new_disposition).await
                }
                .await;

                let cmd = if let Err(e) = r {
                    e.into()
                } else {
                    Command::message(ComposerMessage::RefreshAttachmentList)
                };

                Command::batch([Command::message(Messages::DismissBackgroundProgress), cmd])
            }),
        ])
    }

    /// Add attachment to the draft
    fn refresh_attachment_list(&mut self, context: Arc<MailUserContext>) -> Command<Messages> {
        let metadata_id = self.draft.metadata_id;
        Command::task(async move {
            match async {
                let tether = context.user_stash().connection().await?;
                DraftAttachment::build_list(metadata_id, &tether).await
            }
            .await
            {
                Ok(list) => Command::message(ComposerMessage::AttachmentListRefreshed(list)),
                Err(e) => Command::message(anyhow!(e)),
            }
        })
    }

    fn start_sender_address_change(
        &mut self,
        (email, _): (String, AddressId),
    ) -> Command<Messages> {
        let draft = self.draft.clone();
        let task = Command::task(async move {
            let cmd = match draft.change_sender_address(email).await {
                Ok(_) => match (draft.sender().await, draft.body().await) {
                    (Ok(sender), Ok(body)) => {
                        Command::message(ComposerMessage::FinishChangeAddress { sender, body })
                    }
                    (Err(e), _) | (_, Err(e)) => {
                        Command::message(Messages::DisplayError(None, anyhow::Error::new(e)))
                    }
                },
                Err(e) => Command::message(Messages::DisplayError(
                    Some("Failed to change address".to_owned()),
                    anyhow::Error::new(e),
                )),
            };

            Command::batch([Command::message(Messages::DismissBackgroundProgress), cmd])
        });
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Changing address".into(),
            )),
            task,
        ])
    }

    fn finish_sender_address_change(
        &mut self,
        context: Arc<MailUserContext>,
        sender: String,
        body: &str,
    ) -> Command<Messages> {
        self.sender_input_state = TextInputState::with_value(sender);
        self.text_area = TextArea::new(body.split('\n').map(str::to_owned).collect());
        self.refresh_attachment_list(context)
    }

    fn apply_password_protection(
        &mut self,
        password: SecretString,
        hint: Option<String>,
    ) -> Command<Messages> {
        let draft = self.draft.clone();
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Applying password".to_owned(),
            )),
            Command::task(async move {
                let cmd = match draft
                    .set_password(password.expose_secret().as_str(), hint)
                    .await
                {
                    Ok(()) => Command::message(Messages::DisplayInfo(
                        None,
                        "Password applied successfully".to_owned(),
                    )),
                    Err(e) => Command::message(Messages::DisplayError(
                        None,
                        anyhow!("Failed to apply password: {e}"),
                    )),
                };
                Command::batch([Command::message(Messages::DismissBackgroundProgress), cmd])
            }),
        ])
    }

    fn set_expiration_time(&mut self, expiration_time: DateTime<Local>) -> Command<Messages> {
        let draft = self.draft.clone();
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Setting Expiration Time".to_owned(),
            )),
            Command::task(async move {
                let cmd = match draft
                    .set_expiration_time(DraftExpirationTime::Custom(expiration_time))
                    .await
                {
                    Ok(()) => Command::message(Messages::DisplayInfo(
                        None,
                        format!("Expiration time set to {expiration_time}"),
                    )),
                    Err(e) => Command::message(Messages::DisplayError(
                        None,
                        anyhow!("Failed to set expiration time: {e}"),
                    )),
                };
                Command::batch([Command::message(Messages::DismissBackgroundProgress), cmd])
            }),
        ])
    }
}

struct AttachmentInfo {
    id: LocalAttachmentId,
    disposition: Disposition,
    filename: String,
    state: DraftAttachmentState,
}

impl From<DraftAttachment> for AttachmentInfo {
    fn from(value: DraftAttachment) -> Self {
        Self {
            id: value.metadata.local_id.unwrap(),
            disposition: value.metadata.disposition,
            filename: value.metadata.filename,
            state: value.state,
        }
    }
}
impl Composer {
    #[allow(clippy::too_many_lines)]
    pub fn view(&mut self, frame: &mut Frame, area: Rect) {
        let area = area.inner(Margin {
            horizontal: 4,
            vertical: 2,
        });

        frame.render_widget(Clear {}, area);
        frame.render_widget(Block::new().title("Composer").borders(Borders::ALL), area);

        let area = area.inner(Margin {
            horizontal: 1,
            vertical: 1,
        });

        let area = if let Some(DraftSyncStatus::Cached) = self.draft_sync_status {
            let [error_area, area] =
                Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(area);

            frame.render_widget(
                Block::new()
                    .borders(Borders::ALL)
                    .bg(Color::Red)
                    .fg(Color::White),
                error_area,
            );
            let error_area = error_area.inner(Margin {
                horizontal: 1,
                vertical: 1,
            });
            frame.render_widget(
                Line::from("You are editing a cached version of this draft")
                    .bold()
                    .centered()
                    .bg(Color::Red)
                    .fg(Color::White),
                error_area,
            );

            area
        } else {
            area
        };

        let [
            sender_area,
            to_area,
            cc_area,
            bcc_area,
            subject_area,
            _,
            message_area,
            footer,
        ] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Percentage(100),
            Constraint::Length(1),
        ])
        .areas(area);

        for (title, state, area, input_selection) in [
            ("From: ", &mut self.sender_input_state, sender_area, None),
            (
                "To: ",
                &mut self.to_input_state,
                to_area,
                Some(SelectedInput::To),
            ),
            (
                "CC: ",
                &mut self.cc_input_state,
                cc_area,
                Some(SelectedInput::Cc),
            ),
            (
                "BCC: ",
                &mut self.bcc_input_state,
                bcc_area,
                Some(SelectedInput::Bcc),
            ),
            (
                "Subject: ",
                &mut self.subject_input_state,
                subject_area,
                Some(SelectedInput::Subject),
            ),
        ] {
            frame.render_stateful_widget(TextInput::new(title), area, state);
            if self.recipient_view.is_none()
                && let Some(input_selection) = input_selection
                && input_selection == self.selected_input
            {
                let (x, y) = state.frame_cursor();
                frame.set_cursor_position(Position { x, y });
            }
        }

        let [attachment_list_area, _, body_area] = Layout::horizontal([
            Constraint::Length(20),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .areas(message_area);

        let list = ScrollableList::new(
            List::new(self.attachment_infos.iter().map(|a| {
                Line::from(vec![
                    match a.state {
                        DraftAttachmentState::Uploading => Span::from("U:"),
                        DraftAttachmentState::Uploaded => Span::from("D:"),
                        DraftAttachmentState::Error(_) => Span::from("E:").fg(Color::Red),
                        DraftAttachmentState::Offline => Span::from("O:"),
                        DraftAttachmentState::Pending => Span::from("P:"),
                    }
                    .bold(),
                    Span::from(if a.disposition == Disposition::Inline {
                        "I:"
                    } else {
                        "A:"
                    })
                    .bold(),
                    a.filename.as_str().into(),
                ])
            }))
            .block(Block::new().title("Attachments").borders(Borders::TOP)),
        );
        frame.render_stateful_widget(list, attachment_list_area, &mut self.attachment_list_state);

        frame.render_widget(
            Block::new().borders(Borders::TOP | Borders::LEFT),
            body_area,
        );
        frame.render_widget(&self.text_area, body_area.inner(Margin::new(1, 1)));

        let help_text = vec![
            Span::from(" Esc: ").bold(),
            Span::from("Exit"),
            Span::from(" Tab: ").bold(),
            Span::from("Switch"),
            Span::from(" Shift+Tab: ").bold(),
            Span::from("Switch"),
            Span::from(" Ctrl+s: ").bold(),
            Span::from("Save"),
            Span::from(" Ctrl+d: ").bold(),
            Span::from("Discard"),
            Span::from(" Ctrl+t: ").bold(),
            Span::from("Send"),
            Span::from(" Ctrl+a: ").bold(),
            Span::from("Add Attachment"),
            Span::from(" Ctrl+j: ").bold(),
            Span::from("Schedule"),
            Span::from(" Ctrl+k: ").bold(),
            Span::from("Change Address"),
            Span::from(" Ctrl+p: ").bold(),
            Span::from("Password"),
        ];
        frame.render_widget(Block::new().style(Style::new().reversed()), footer);
        frame.render_widget(Line::from(help_text), footer);

        // Render recipient list as overlay LAST so it appears on top of everything
        if let Some(recipient_list) = &mut self.recipient_view {
            let field_area = match recipient_list.group_id() {
                RecipientGroupId::To => to_area,
                RecipientGroupId::Cc => cc_area,
                RecipientGroupId::Bcc => bcc_area,
            };
            // Start overlay at the field, extend into message area
            let overlay_bottom = message_area.y + message_area.height / 2;
            let overlay_area = Rect::new(
                field_area.x,
                field_area.y,
                field_area.width,
                overlay_bottom - field_area.y,
            );
            recipient_list.view(frame, overlay_area);
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn handle_event(
        &mut self,
        ctx: &Arc<MailUserContext>,
        _: &Mailbox,
        event: Event,
    ) -> Command<Messages> {
        if let Some(recipient_view) = &mut self.recipient_view {
            return recipient_view.handle_event(&event);
        }

        if let Event::Key(key) = &event {
            match key.code {
                KeyCode::Esc => return Command::message(Message::CloseComposer),
                KeyCode::Tab => {
                    match self.selected_input {
                        SelectedInput::To => {
                            self.to_input_state.set_selected(false);
                            self.selected_input = SelectedInput::Cc;
                            self.cc_input_state.set_selected(true);
                        }
                        SelectedInput::Cc => {
                            self.cc_input_state.set_selected(false);
                            self.selected_input = SelectedInput::Bcc;
                            self.bcc_input_state.set_selected(true);
                        }
                        SelectedInput::Bcc => {
                            self.bcc_input_state.set_selected(false);
                            self.selected_input = SelectedInput::Subject;
                            self.subject_input_state.set_selected(true);
                        }
                        SelectedInput::Subject => {
                            self.subject_input_state.set_selected(false);
                            self.selected_input = SelectedInput::Body;
                        }
                        SelectedInput::Body => {
                            self.selected_input = SelectedInput::Attachments;
                            self.attachment_list_state.select(Some(0));
                        }
                        SelectedInput::Attachments => {
                            self.selected_input = SelectedInput::To;
                            self.attachment_list_state.select(None);
                            self.to_input_state.set_selected(true);
                        }
                    }
                    return Command::none();
                }
                KeyCode::Char('s') => {
                    if self.selected_input == SelectedInput::Attachments {
                        if let Some(index) = self.attachment_list_state.selected() {
                            return Command::message(ComposerMessage::SwapDisposition(
                                self.attachment_infos[index].id,
                            ));
                        }
                    } else if key.modifiers.contains(KeyModifiers::CONTROL) {
                        return Command::message(ComposerMessage::Save);
                    }
                }
                KeyCode::Char('t') => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        return Command::message(ComposerMessage::Send);
                    }
                }
                KeyCode::Char('j') => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        let ctx = ctx.clone();
                        return Command::task(async move {
                            match Draft::schedule_send_options(&ctx).await {
                                Ok(options) => Command::message(Messages::raise_popup(
                                    ScheduleSendPopup::new(options),
                                )),
                                Err(e) => Command::message(Messages::DisplayError(
                                    None,
                                    anyhow!("Failed to retrieve schedule send options: {e:?}"),
                                )),
                            }
                        });
                    }
                }
                KeyCode::Char('a') => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        return Command::message(Messages::select_file_path(|path| {
                            Command::message(ComposerMessage::CreateAttachment(path.to_path_buf()))
                        }));
                    }
                }
                KeyCode::Char('d') => {
                    if self.selected_input == SelectedInput::Attachments {
                        if let Some(index) = self.attachment_list_state.selected() {
                            return Command::message(ComposerMessage::RemoveAttachment(
                                self.attachment_infos[index].id,
                            ));
                        }
                    } else if key.modifiers.contains(KeyModifiers::CONTROL) {
                        return Command::message(ComposerMessage::Discard);
                    }
                }
                KeyCode::Char('r') => {
                    if self.selected_input == SelectedInput::Attachments
                        && let Some(index) = self.attachment_list_state.selected()
                    {
                        return Command::message(ComposerMessage::RetryAttachmentOp(
                            self.attachment_infos[index].id,
                        ));
                    }
                }
                KeyCode::Char('k') => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        return AddressListPopup::open(self.draft.clone());
                    }
                }
                KeyCode::Char('p') => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        return PasswordProtectPopup::open();
                    }
                }
                KeyCode::Char('e') => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        return ExpirationTimePopup::open();
                    }
                }
                _ => {}
            }
        }
        match self.selected_input {
            SelectedInput::To => {
                if let Event::Key(key) = &event {
                    let initial_char = if let KeyCode::Char(c) = key.code {
                        Some(c)
                    } else {
                        None
                    };
                    return Command::message(ComposerMessage::OpenRecipientList(
                        RecipientGroupId::To,
                        initial_char,
                    ));
                }
            }
            SelectedInput::Cc => {
                if let Event::Key(key) = &event {
                    let initial_char = if let KeyCode::Char(c) = key.code {
                        Some(c)
                    } else {
                        None
                    };
                    return Command::message(ComposerMessage::OpenRecipientList(
                        RecipientGroupId::Cc,
                        initial_char,
                    ));
                }
            }
            SelectedInput::Bcc => {
                if let Event::Key(key) = &event {
                    let initial_char = if let KeyCode::Char(c) = key.code {
                        Some(c)
                    } else {
                        None
                    };
                    return Command::message(ComposerMessage::OpenRecipientList(
                        RecipientGroupId::Bcc,
                        initial_char,
                    ));
                }
            }
            SelectedInput::Subject => {
                self.subject_input_state.handle_event(&event);
            }
            SelectedInput::Body => {
                self.text_area.input(tui_textarea::Input::from(event));
            }
            SelectedInput::Attachments => {
                if let Event::Key(key) = &event {
                    self.attachment_list_state.handle_event(key.code);
                }
            }
        }

        Command::none()
    }

    pub fn update(
        &mut self,
        user_ctx: &Arc<MailUserContext>,
        message: Message,
    ) -> Command<Messages> {
        let Message::Composer(message) = message else {
            return Command::none();
        };

        match message {
            ComposerMessage::Save => self.save(),
            ComposerMessage::Send => self.send(None),
            ComposerMessage::ScheduleSend(delivery_time) => self.send(Some(delivery_time)),
            ComposerMessage::Discard => self.discard(),
            ComposerMessage::CreateAttachment(path) => {
                self.create_attachment(user_ctx.to_owned(), path)
            }
            ComposerMessage::AddAttachment(attachment) => self.add_attachment(attachment),
            ComposerMessage::RefreshAttachmentList => {
                self.refresh_attachment_list(user_ctx.to_owned())
            }
            ComposerMessage::AttachmentListRefreshed(list) => {
                self.attachment_infos = list.into_iter().map(AttachmentInfo::from).collect();
                Command::none()
            }
            ComposerMessage::RemoveAttachment(id) => self.remove_attachment(id),
            ComposerMessage::StartChangeAddress(email_address_id) => {
                self.start_sender_address_change(email_address_id)
            }
            ComposerMessage::FinishChangeAddress { sender, body } => {
                self.finish_sender_address_change(user_ctx.to_owned(), sender, &body)
            }
            ComposerMessage::SetPasswordProtection(password, hint) => {
                self.apply_password_protection(password, hint)
            }
            ComposerMessage::SetExpirationTime(dt) => self.set_expiration_time(dt),
            ComposerMessage::RetryAttachmentOp(id) => self.retry_attachment_op(id),
            ComposerMessage::SwapDisposition(id) => {
                self.swap_attachment_disposition(user_ctx.to_owned(), id)
            }
            ComposerMessage::OpenRecipientList(recipient_group_id, initial_char) => {
                TuiRecipientList::open(self.draft.clone(), recipient_group_id, initial_char)
            }
            ComposerMessage::ShowRecipientList(tui_recipient_list) => {
                self.recipient_view = Some(tui_recipient_list);
                Command::none()
            }
            ComposerMessage::RecipientList(recipient_list_message) => {
                if let RecipientListMessage::UpdateRecipients(group, recipients) =
                    &recipient_list_message
                {
                    let value =
                        TextInputState::with_value(recipient_list_to_display_value(recipients));

                    match *group {
                        RecipientGroupId::To => self.to_input_state = value,
                        RecipientGroupId::Cc => self.cc_input_state = value,
                        RecipientGroupId::Bcc => self.bcc_input_state = value,
                    }
                }
                if let Some(recipient_list) = &mut self.recipient_view {
                    recipient_list.update(recipient_list_message)
                } else {
                    Command::none()
                }
            }
            ComposerMessage::CloseRecipientList => {
                self.recipient_view = None;
                Command::None
            }
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

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum SelectedInput {
    To,
    Cc,
    Bcc,
    Subject,
    Body,
    Attachments,
}

fn recipient_list_to_display_value(list: &RecipientList) -> String {
    list.to_message_recipients()
        .into_iter()
        .map(|v| v.address.into_clear_text_string())
        .collect::<Vec<_>>()
        .join(", ")
}

struct ComposerDraftState {
    subject: String,
    body: String,
}

impl ComposerDraftState {
    async fn apply(self, draft: &Draft) -> Result<(), MailContextError> {
        // We are TUI, what else can we do?
        draft.set_mime_type(MessageMimeType::TextPlain).await?;
        draft.set_subject(self.subject).await?;
        draft.set_body(self.body).await?;
        draft.save().await?;
        Ok(())
    }
}

fn draft_options() -> DraftActorOptions {
    DraftActorOptions {
        address_validation_enabled: true,
        auto_save_every: None,
    }
}
