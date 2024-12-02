use crate::app::Command;
use crate::app_model::mailbox::model::StateHandler;
use crate::app_model::mailbox::{ComposerMessage, Message};
use crate::messages::Messages;
use crate::widgets::{TextInput, TextInputState};
use crossterm::event::{KeyCode, KeyModifiers};
use proton_core_common::datatypes::LocalId;
use proton_mail_common::actions::draft::Save;
use proton_mail_common::datatypes::{Disposition, MimeType};
use proton_mail_common::draft::{recipients, Draft, ReplyMode};
use proton_mail_common::models::MailSettings;
use proton_mail_common::{MailContext, MailContextError, MailUserContext, Mailbox};
use ratatui::crossterm::event::Event;
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List};
use ratatui::Frame;
use std::io::Cursor;
use std::sync::Arc;
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
    attachment_infos: Vec<AttachmentInfo>,
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
                    match Draft::empty(ctx.user_stash()).await {
                        Ok(draft) => {
                            Command::message(Message::OpenComposer(Composer::new(draft)).into())
                        }
                        Err(e) => {
                            error!("Failed to create new draft:{e}");
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
        message_id: LocalId,
        reply_mode: ReplyMode,
    ) -> Command<Messages> {
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Creating draft reply...".to_owned(),
            )),
            Command::task(async move {
                Command::batch([
                    Command::message(Messages::DismissBackgroundProgress),
                    match Draft::reply(&context, message_id, reply_mode, false).await {
                        Ok(draft) => {
                            Command::message(Message::OpenComposer(Composer::new(draft)).into())
                        }
                        Err(e) => {
                            error!("Failed to open message in composer: {e}");
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
    pub fn open(context: Arc<MailUserContext>, message_id: LocalId) -> Command<Messages> {
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Opening draft...".to_owned(),
            )),
            Command::task(async move {
                Command::batch([
                    Command::message(Messages::DismissBackgroundProgress),
                    match Draft::open(&context, message_id).await {
                        Ok(draft) => {
                            Command::message(Message::OpenComposer(Composer::new(draft)).into())
                        }
                        Err(e) => {
                            error!("Failed to open message in composer: {e}");
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
                    match context.queue().queue_action(save_action).await {
                        Ok(_) => Command::none(),
                        Err(e) => {
                            error!("Failed to save draft: {e}");
                            Command::message(MailContextError::from(e).into())
                        }
                    },
                ])
            }),
        ])
    }

    fn create_save_action(&mut self) -> Result<Save, recipients::Error> {
        // We are TUI, what else can we do?
        self.draft.mime_type = MimeType::TextPlain;
        self.draft.subject = self.subject_input_state.value().to_owned();
        self.draft.body = self.text_area.lines().join("\n");
        self.draft.cc_list = recipients_value_to_list(self.cc_input_state.value())?;
        self.draft.bcc_list = recipients_value_to_list(self.bcc_input_state.value())?;
        self.draft.to_list = recipients_value_to_list(self.to_input_state.value())?;
        Ok(self.draft.to_save_action())
    }

    /// Send the draft.
    fn send(&mut self, context: Arc<MailUserContext>) -> Command<Messages> {
        let save_action = match self.create_save_action() {
            Ok(action) => action,
            Err(err) => {
                return Command::message(Messages::DisplayError(
                    Some("Invalid recipient".to_owned()),
                    err.into(),
                ));
            }
        };
        match self.draft.to_send_action() {
            Ok(send_action) => Command::batch([
                Command::message(Messages::DisplayBackgroundProgress(
                    "Sending draft...".to_owned(),
                )),
                Command::task(async move {
                    Command::batch([
                        Command::message(Messages::DismissBackgroundProgress),
                        match Draft::send(context.queue(), save_action, send_action).await {
                            Ok(()) => Command::message(Message::CloseComposer.into()),
                            Err(e) => {
                                error!("Failed to save draft: {e}");
                                Command::message(e.into())
                            }
                        },
                    ])
                }),
            ]),
            Err(e) => Command::message(MailContextError::from(e).into()),
        }
    }

    fn new(draft: Draft) -> Self {
        let sender = draft.sender.clone();
        let to_list = recipient_list_to_display_value(&draft.to_list);
        let cc_list = recipient_list_to_display_value(&draft.cc_list);
        let bcc_list = recipient_list_to_display_value(&draft.bcc_list);
        let text_area = if draft.mime_type == MimeType::TextHtml {
            let config = html2text::config::plain();
            let cursor = Cursor::new(&draft.body);
            let text = config
                .string_from_read(cursor, 80)
                .unwrap_or_else(|e| format!("Failed to parse html:{e}"));
            TextArea::new(text.split('\n').map(str::to_owned).collect())
        } else if draft.mime_type == MimeType::TextPlain {
            TextArea::new(draft.body.split('\n').map(str::to_owned).collect())
        } else {
            TextArea::new(vec!["Unknown mime type".to_owned()])
        };
        let subject = draft.subject.clone();
        let attachment_infos = draft
            .attachments
            .iter()
            .map(|attachment| AttachmentInfo {
                disposition: attachment.disposition,
                filename: attachment.filename.clone(),
            })
            .collect();
        Self {
            draft,
            text_area,
            selected_input: SelectedInput::To,
            sender_input_state: TextInputState::with_value(sender),
            to_input_state: TextInputState::with_value(to_list).selected(true),
            cc_input_state: TextInputState::with_value(cc_list),
            bcc_input_state: TextInputState::with_value(bcc_list),
            subject_input_state: TextInputState::with_value(subject),
            attachment_infos,
        }
    }
}

struct AttachmentInfo {
    disposition: Disposition,
    filename: String,
}
impl StateHandler for Composer {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
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

        let [sender_area, to_area, cc_area, bcc_area, subject_area, _, message_area, footer] =
            Layout::vertical([
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

        frame.render_widget(
            List::new(self.attachment_infos.iter().map(|a| {
                Line::from(vec![
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
            attachment_list_area,
        );

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
            Span::from("Send"),
        ];
        frame.render_widget(Block::new().style(Style::new().reversed()), footer);
        frame.render_widget(Line::from(help_text), footer);
    }

    fn handle_event(&mut self, _: &Mailbox, event: Event) -> Command<Messages> {
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
                            self.selected_input = SelectedInput::To;
                            self.to_input_state.set_selected(true);
                        }
                    };
                    return Command::none();
                }
                KeyCode::Char('s') => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        return Command::message(ComposerMessage::Save.into());
                    }
                }
                KeyCode::Char('d') => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        return Command::message(ComposerMessage::Send.into());
                    }
                }
                _ => {}
            };
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
        let Message::Composer(message) = message else {
            return Command::none();
        };

        match message {
            ComposerMessage::Save => self.save(mbox.user_context()),
            ComposerMessage::Send => self.send(mbox.user_context()),
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum SelectedInput {
    To,
    Cc,
    Bcc,
    Subject,
    Body,
}

fn recipients_value_to_list(recipients: &str) -> Result<recipients::List, recipients::Error> {
    let mut list = recipients::List::default();
    for addr in recipients.split(',') {
        list.add_single(recipients::Entry {
            email: addr.to_owned(),
            display_name: None,
        })?;
    }
    Ok(list)
}

fn recipient_list_to_display_value(list: &recipients::List) -> String {
    list.to_message_recipients()
        .into_iter()
        .map(|v| v.address)
        .collect::<Vec<_>>()
        .join(", ")
}
