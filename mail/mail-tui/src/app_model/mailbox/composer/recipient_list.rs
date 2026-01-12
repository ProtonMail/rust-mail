use crate::app::Command;
use crate::app_model::mailbox::{ComposerMessage, RecipientListMessage};
use crate::messages::Messages;
use crate::widgets::lock_icon::lock_icon_to_text;
use crate::widgets::utils::ScrollableState;
use crate::widgets::{ScrollableList, ScrollableListState, TextInput, TextInputState};
use anyhow::anyhow;
use crossterm::event::{Event, KeyCode};
use proton_mail_common::draft::recipients::{Recipient, RecipientEntry, RecipientList};
use proton_mail_common::draft::{Draft, RecipientGroupId, recipients};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, List, ListItem};

enum Selection {
    Text,
    List,
}

pub struct TuiRecipientList {
    draft: Draft,
    group_id: RecipientGroupId,
    recipients: RecipientList,
    scrollable_list_state: ScrollableListState,
    text_state: TextInputState,
    selection: Selection,
}

impl TuiRecipientList {
    fn new(draft: Draft, recipients: Vec<Recipient>, group_id: RecipientGroupId) -> Self {
        let scrollable_list_state = ScrollableListState::new((!recipients.is_empty()).then_some(0));
        Self {
            draft,
            recipients: RecipientList::with(recipients),
            group_id,
            text_state: TextInputState::new().selected(true),
            scrollable_list_state,
            selection: Selection::Text,
        }
    }

    pub fn open(draft: Draft, group_id: RecipientGroupId) -> Command<Messages> {
        Command::batch([
            Command::message(Messages::DisplayBackgroundProgress(
                "Loading Recipient List".to_owned(),
            )),
            Command::task(async move {
                let cmd = match draft.recipients(group_id).await {
                    Ok(recipients) => Command::message(ComposerMessage::ShowRecipientList(
                        Self::new(draft, recipients, group_id),
                    )),
                    Err(e) => Command::message(Messages::DisplayError(
                        None,
                        anyhow!("Failed to load recipients: {e:?}"),
                    )),
                };

                Command::batch([Command::message(Messages::DismissBackgroundProgress), cmd])
            }),
        ])
    }

    pub fn handle_event(&mut self, event: &Event) -> Command<Messages> {
        let Event::Key(key) = &event else {
            return Command::none();
        };
        match key.code {
            KeyCode::Esc => {
                return Command::message(ComposerMessage::CloseRecipientList);
            }
            KeyCode::Tab => {
                return Command::message(RecipientListMessage::ChangeSelection);
            }
            _ => {}
        }

        match self.selection {
            Selection::Text if key.code == KeyCode::Enter => Command::message(
                RecipientListMessage::AddRecipient(self.text_state.value().to_owned()),
            ),
            Selection::List => {
                if let KeyCode::Char('d') = key.code {
                    self.scrollable_list_state
                        .selected()
                        .map_or(Command::none(), |index| {
                            Command::message(RecipientListMessage::DeleteRecipient(
                                self.recipients.recipients()[index].clone(),
                            ))
                        })
                } else {
                    self.scrollable_list_state.handle_event(key.code);
                    Command::none()
                }
            }
            Selection::Text => {
                self.text_state.handle_event(event);
                Command::none()
            }
        }
    }

    pub fn update(&mut self, message: RecipientListMessage) -> Command<Messages> {
        match message {
            RecipientListMessage::ChangeSelection => {
                self.selection = match self.selection {
                    Selection::Text => {
                        self.text_state.set_selected(false);
                        self.scrollable_list_state
                            .select((!self.recipients.is_empty()).then_some(0));
                        Selection::List
                    }
                    Selection::List => {
                        self.text_state.set_selected(true);
                        self.scrollable_list_state.select(None);
                        Selection::Text
                    }
                };

                Command::none()
            }
            RecipientListMessage::AddRecipient(email) => {
                let draft = self.draft.clone();
                let group = self.group_id;
                self.text_state.reset();
                Command::task(async move {
                    match draft
                        .add_single_recipient(group, RecipientEntry::new(&email))
                        .await
                    {
                        Ok(()) => Command::none(),
                        Err(e) => {
                            Command::message(Messages::DisplayError(None, anyhow::Error::new(e)))
                        }
                    }
                })
            }
            RecipientListMessage::DeleteRecipient(recipient) => {
                let draft = self.draft.clone();
                let group = self.group_id;
                Command::task(async move {
                    let r = match recipient {
                        Recipient::Single(single) => {
                            draft.remove_single_recipient(group, single.email).await
                        }
                        Recipient::Group(recipient_group) => {
                            draft
                                .remove_recipient_group(group, recipient_group.group_name)
                                .await
                        }
                    };
                    match r {
                        Ok(()) => Command::none(),
                        Err(e) => {
                            Command::message(Messages::DisplayError(None, anyhow::Error::new(e)))
                        }
                    }
                })
            }

            RecipientListMessage::UpdateRecipients(group_id, recipients) => {
                if group_id == self.group_id {
                    self.recipients = recipients;
                }
                Command::none()
            }
        }
    }

    pub fn view(&mut self, frame: &mut Frame, area: Rect) {
        let area = area.inner(Margin::new(1, 1));

        let [input_area, list_area] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(3)]).areas(area);

        frame.render_stateful_widget(TextInput::new("email:"), input_area, &mut self.text_state);
        if let Selection::Text = self.selection {
            let (x, y) = self.text_state.frame_cursor();
            frame.set_cursor_position(Position { x, y });
        }

        let list = ScrollableList::new(
            List::new(
                self.recipients
                    .recipients()
                    .iter()
                    .map(|recipient| match recipient {
                        Recipient::Single(single) => {
                            let span_style = match single.state {
                                recipients::ValidationState::Valid { official, proton } => {
                                    if official {
                                        Style::new().bg(Color::LightMagenta)
                                    } else if proton {
                                        Style::new().bg(Color::Magenta)
                                    } else {
                                        Style::new().bg(Color::Green)
                                    }
                                    .fg(Color::White)
                                }
                                recipients::ValidationState::DoesNotExist
                                | recipients::ValidationState::InvalidEmail => {
                                    Style::new().bg(Color::Red).fg(Color::White)
                                }
                                recipients::ValidationState::Unchecked
                                | recipients::ValidationState::Validating
                                | recipients::ValidationState::Unknown => Style::default(),
                            };

                            let (lock_str, lock_style) =
                                lock_icon_to_text(single.privacy_lock.into());

                            let text = Text::from(Line::from(vec![
                                Span::from(lock_str).style(lock_style),
                                Span::from(" "),
                                Span::from(single.email.as_clear_text_str()).style(span_style),
                            ]));

                            ListItem::new(text)
                        }
                        Recipient::Group(group) => ListItem::from(format!(
                            "{} ({}/{})",
                            group.group_name.as_str(),
                            group.recipients.len(),
                            group.total_in_group
                        )),
                    }),
            )
            .highlight_symbol("> ")
            .highlight_style(Style::default())
            .block(Block::bordered()),
        );

        frame.render_stateful_widget(list, list_area, &mut self.scrollable_list_state);
    }
}
