mod schedule_send;

use crate::app::Command;
use crate::app_model::YesNoPopup;
use crate::app_model::mailbox::composer::schedule_send::ScheduleSendPopup;
use crate::app_model::mailbox::{ComposerMessage, Message};
use crate::messages::Messages;
use crate::widgets::{ScrollableList, ScrollableListState, TextInput, TextInputState};
use anyhow::anyhow;
use chrono::{DateTime, Local};
use crossterm::event::{KeyCode, KeyModifiers};
use futures::FutureExt;
use proton_mail_common::datatypes::{Disposition, LocalAttachmentId, LocalMessageId, MimeType};
use proton_mail_common::draft::attachments::{DraftAttachment, DraftAttachmentState};
use proton_mail_common::draft::observers::DraftAttachmentObserver;
use proton_mail_common::draft::recipients::MaybeEmptyString;
use proton_mail_common::draft::{
    Draft, DraftSaveActionQueuer, DraftSyncStatus, ReplyMode, recipients,
};
use proton_mail_common::models::{Attachment, MetadataId};
use proton_mail_common::{MailContextError, MailUserContext, Mailbox};
use proton_mail_html_transformer::Html2TextOptions;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List};
use stash::stash::{Stash, StashError, Tether};
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::error;
use tui_textarea::TextArea;

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
                    match Draft::empty(&ctx).await {
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
                    match Draft::reply(&context, message_id, reply_mode, false, None).await {
                        Ok(draft) => {
                            Composer::create(draft, None, context.user_stash().clone()).await
                        }
                        Err(e) => {
                            error!("Failed to open message in composer: {e:?}");
                            Command::batch([
                                Command::message(Message::CloseComposer.into()),
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
                    match Draft::open(&context, message_id).await {
                        Ok((draft, sync_status)) => {
                            Composer::create(draft, Some(sync_status), context.user_stash().clone())
                                .await
                        }
                        Err(e) => {
                            error!("Failed to open message in composer: {e:?}");
                            Command::batch([
                                Command::message(Message::CloseComposer.into()),
                                Command::message(Messages::from(e)),
                            ])
                        }
                    },
                ])
            }),
        ])
    }

    /// Save a draft.
    fn save(&mut self, context: Arc<MailUserContext>) -> Command<Messages> {
        let save_action = match self.create_save_action() {
            Ok(action) => action,
            Err(err) => {
                return Command::message(Messages::DisplayError(
                    Some("Invalid recipient".to_owned()),
                    err.into(),
                ));
            }
        };
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Saving draft...".to_owned(),
            )),
            Command::task(async move {
                Command::batch([
                    Command::message(Messages::DismissBackgroundProgress),
                    match save_action
                        .queue(context.action_queue(), &context.user_stash().connection())
                        .await
                    {
                        Ok(_) => Command::none(),
                        Err(e) => {
                            error!("Failed to save draft: {e:?}");
                            Command::message(e.into())
                        }
                    },
                ])
            }),
        ])
    }

    fn update_draft_from_state(&mut self) -> Result<(), recipients::RecipientError> {
        // We are TUI, what else can we do?
        self.draft.set_mime_type(MimeType::TextPlain);
        self.draft.subject = self.subject_input_state.value().to_owned();
        self.draft.set_body(self.text_area.lines().join("\n"));
        self.draft.cc_list = recipients_value_to_list(self.cc_input_state.value())?;
        self.draft.bcc_list = recipients_value_to_list(self.bcc_input_state.value())?;
        self.draft.to_list = recipients_value_to_list(self.to_input_state.value())?;
        Ok(())
    }

    fn create_save_action(&mut self) -> Result<DraftSaveActionQueuer, recipients::RecipientError> {
        self.update_draft_from_state()?;
        Ok(self.draft.to_save_action())
    }

    /// Send the draft.
    fn send(
        &mut self,
        scheduled_time: Option<DateTime<Local>>,
        context: Arc<MailUserContext>,
    ) -> Command<Messages> {
        if let Err(err) = self.update_draft_from_state() {
            return Command::message(Messages::DisplayError(
                Some("Invalid recipient".to_owned()),
                err.into(),
            ));
        }
        match if let Some(scheduled_time) = scheduled_time {
            self.draft.to_schedule_send_action(scheduled_time)
        } else {
            self.draft.to_send_action()
        } {
            Ok(send_action) => Command::batch([
                Command::message(Messages::DisplayBackgroundProgress(
                    "Sending draft...".to_owned(),
                )),
                Command::task(async move {
                    Command::batch([
                        Command::message(Messages::DismissBackgroundProgress),
                        match send_action
                            .queue(context.action_queue(), &context.user_stash().connection())
                            .await
                        {
                            Ok(_) => Command::message(Message::CloseComposer.into()),
                            Err(e) => {
                                error!("Failed to save draft: {e:?}");
                                Command::message(e.into())
                            }
                        },
                    ])
                }),
            ]),
            Err(e) => Command::message(MailContextError::from(e).into()),
        }
    }
    async fn create(
        draft: Draft,
        sync_status: Option<DraftSyncStatus>,
        stash: Stash,
    ) -> Command<Messages> {
        match Self::new_impl(draft, sync_status, stash).await {
            Ok((composer, background_cmd)) => Command::batch([
                Command::message(Message::OpenComposer(composer).into()),
                background_cmd,
            ]),
            Err(e) => Command::message(Messages::DisplayError(
                Some("Open Composer failed".to_owned()),
                e.into(),
            )),
        }
    }

    async fn new_impl(
        draft: Draft,
        sync_status: Option<DraftSyncStatus>,
        stash: Stash,
    ) -> Result<(Self, Command<Messages>), StashError> {
        let sender = draft.sender.clone();
        let to_list = recipient_list_to_display_value(&draft.to_list);
        let cc_list = recipient_list_to_display_value(&draft.cc_list);
        let bcc_list = recipient_list_to_display_value(&draft.bcc_list);
        let text_area = if draft.mime_type() == MimeType::TextHtml {
            let text = proton_mail_html_transformer::Transformer::html2text_str(
                draft.body(),
                Html2TextOptions::default(),
            )
            .unwrap_or_else(|e| format!("Failed to parse html:{e}"));
            TextArea::new(text.split('\n').map(str::to_owned).collect())
        } else if draft.mime_type() == MimeType::TextPlain {
            TextArea::new(draft.body().split('\n').map(str::to_owned).collect())
        } else {
            TextArea::new(vec!["Unknown mime type".to_owned()])
        };

        let subject = draft.subject.clone();
        let tether = stash.connection();
        let attachment_infos = Self::build_attachment_infos(draft.metadata_id, &tether).await?;
        drop(tether);

        let mut observer = DraftAttachmentObserver::new(draft.metadata_id, stash).await?;
        let cancellation_token = CancellationToken::new();
        let cancellation_token_cloned = cancellation_token.clone();
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
                              let _ = sender.send_async(Command::message(ComposerMessage::RefreshAttachmentList.into())).await;
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
                sender_input_state: TextInputState::with_value(sender),
                to_input_state: TextInputState::with_value(to_list).selected(true),
                cc_input_state: TextInputState::with_value(cc_list),
                bcc_input_state: TextInputState::with_value(bcc_list),
                subject_input_state: TextInputState::with_value(subject),
                attachment_list_state: ScrollableListState::new(None),
                attachment_infos,
                draft_sync_status: sync_status,
                _observer_cancellation_token: cancellation_token,
            },
            background_cmd,
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
    fn discard(&mut self, context: Arc<MailUserContext>) -> Command<Messages> {
        let discard_action = self.draft.to_discard_action();
        let popup = YesNoPopup::new(
            "Discard Draft",
            "Are you sure you wish to discard the current draft?",
        )
        .on_accept(Command::batch([
            Command::message(Message::CloseComposer.into()),
            Command::message(Messages::DisplayBackgroundProgress(
                "Discarding Draft".to_owned(),
            )),
            Command::task(async move {
                let cmd = match discard_action.queue(context.action_queue()).await {
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
        let address_id = self.draft.address_id.clone();
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Preparing Attachment".to_owned(),
            )),
            Command::task(async move {
                let mut tether = context.user_stash().connection();
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
                    Ok(attachment) => Command::message(
                        ComposerMessage::AddAttachment(Box::new(attachment)).into(),
                    ),
                    Err(e) => Command::message(anyhow::Error::new(e).into()),
                };

                Command::batch([Command::message(Messages::DismissBackgroundProgress), cmd])
            }),
        ])
    }

    /// Add attachment to the draft
    fn add_attachment(
        &mut self,
        context: Arc<MailUserContext>,
        attachment: Attachment,
    ) -> Command<Messages> {
        // Note that we want to make sure the action is queued first
        // before we allow the user to send or we can run into missing depencency issues.
        let action = self.draft.to_add_attachment_action(attachment);
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Adding Attachment to message".to_owned(),
            )),
            Command::task(async move {
                let tether = context.user_stash().connection();
                let cmd = if let Err(e) = action.queue(context.action_queue(), &tether).await {
                    Command::message(anyhow::Error::new(e).into())
                } else {
                    Command::message(ComposerMessage::RefreshAttachmentList.into())
                };

                Command::batch([Command::message(Messages::DismissBackgroundProgress), cmd])
            }),
        ])
    }

    /// Remove an attachment from the draft
    fn remove_attachment(
        &mut self,
        context: Arc<MailUserContext>,
        id: LocalAttachmentId,
    ) -> Command<Messages> {
        let action = self.draft.to_remove_attachment_action(id);
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Removing Attachment from message".to_owned(),
            )),
            Command::task(async move {
                let tether = context.user_stash().connection();
                let cmd = if let Err(e) = action.queue(context.action_queue(), &tether).await {
                    Command::message(anyhow::Error::new(e).into())
                } else {
                    Command::message(ComposerMessage::RefreshAttachmentList.into())
                };

                Command::batch([Command::message(Messages::DismissBackgroundProgress), cmd])
            }),
        ])
    }

    /// Add attachment to the draft
    fn refresh_attachment_list(&mut self, context: Arc<MailUserContext>) -> Command<Messages> {
        let metadata_id = self.draft.metadata_id;
        Command::task(async move {
            let tether = context.user_stash().connection();
            match DraftAttachment::build_list(metadata_id, &tether).await {
                Ok(list) => Command::message(ComposerMessage::AttachmentListRefreshed(list).into()),
                Err(e) => Command::message(anyhow::Error::new(e).into()),
            }
        })
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
            if let Some(input_selection) = input_selection {
                if input_selection == self.selected_input {
                    let (x, y) = state.frame_cursor();
                    frame.set_cursor_position(Position { x, y });
                }
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
        ];
        frame.render_widget(Block::new().style(Style::new().reversed()), footer);
        frame.render_widget(Line::from(help_text), footer);
    }

    #[allow(clippy::too_many_lines)]
    pub fn handle_event(
        &mut self,
        ctx: &Arc<MailUserContext>,
        _: &Mailbox,
        event: Event,
    ) -> Command<Messages> {
        if let Event::Key(key) = &event {
            match key.code {
                KeyCode::Esc => return Command::message(Message::CloseComposer.into()),
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
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        return Command::message(ComposerMessage::Save.into());
                    }
                }
                KeyCode::Char('t') => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        return Command::message(ComposerMessage::Send.into());
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
                            Command::message(
                                ComposerMessage::CreateAttachment(path.to_path_buf()).into(),
                            )
                        }));
                    }
                }
                KeyCode::Char('d') => {
                    if self.selected_input == SelectedInput::Attachments {
                        if let Some(index) = self.attachment_list_state.selected() {
                            return Command::message(
                                ComposerMessage::RemoveAttachment(self.attachment_infos[index].id)
                                    .into(),
                            );
                        }
                    } else if key.modifiers.contains(KeyModifiers::CONTROL) {
                        return Command::message(ComposerMessage::Discard.into());
                    }
                }
                _ => {}
            }
        }
        match self.selected_input {
            SelectedInput::To => {
                self.to_input_state.handle_event(&event);
            }
            SelectedInput::Cc => {
                self.cc_input_state.handle_event(&event);
            }
            SelectedInput::Bcc => {
                self.bcc_input_state.handle_event(&event);
            }
            SelectedInput::Subject => {
                self.subject_input_state.handle_event(&event);
            }
            SelectedInput::Body => {
                self.text_area.input(tui_textarea::Input::from(event));
            }
            SelectedInput::Attachments => {
                if let Event::Key(key) = &event {
                    match key.code {
                        KeyCode::Char('k') | KeyCode::Up => {
                            self.attachment_list_state.prev();
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            self.attachment_list_state.next();
                        }
                        _ => {}
                    }
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
            ComposerMessage::Save => self.save(user_ctx.to_owned()),
            ComposerMessage::Send => self.send(None, user_ctx.to_owned()),
            ComposerMessage::ScheduleSend(delivery_time) => {
                self.send(Some(delivery_time), user_ctx.to_owned())
            }
            ComposerMessage::Discard => self.discard(user_ctx.to_owned()),
            ComposerMessage::CreateAttachment(path) => {
                self.create_attachment(user_ctx.to_owned(), path)
            }
            ComposerMessage::AddAttachment(attachment) => {
                self.add_attachment(user_ctx.to_owned(), *attachment)
            }
            ComposerMessage::RefreshAttachmentList => {
                self.refresh_attachment_list(user_ctx.to_owned())
            }
            ComposerMessage::AttachmentListRefreshed(list) => {
                self.attachment_infos = list.into_iter().map(AttachmentInfo::from).collect();
                Command::none()
            }
            ComposerMessage::RemoveAttachment(id) => {
                self.remove_attachment(user_ctx.to_owned(), id)
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

fn recipients_value_to_list(
    recipients: &str,
) -> Result<recipients::RecipientList, recipients::RecipientError> {
    let mut list = recipients::RecipientList::default();
    for addr in recipients.split(',') {
        list.add_single(recipients::RecipientEntry {
            email: addr.to_owned(),
            display_name: MaybeEmptyString(None),
        })?;
    }
    Ok(list)
}

fn recipient_list_to_display_value(list: &recipients::RecipientList) -> String {
    list.to_message_recipients()
        .into_iter()
        .map(|v| v.address)
        .collect::<Vec<_>>()
        .join(", ")
}
