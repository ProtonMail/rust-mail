use crossterm::event::{Event, KeyCode};
use futures::FutureExt;
use itertools::Itertools;
use proton_core_common::{
    datatypes::{ContactItemType, GroupedContacts},
    models::Contact,
};
use proton_mail_common::MailUserContext;
use ratatui::{
    layout::{Constraint, Flex, Layout, Margin},
    prelude::Rect,
    style::{Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Cell, List, ListItem, Row, Table},
    Frame,
};
use stash::stash::{Tether, WatcherHandle};
use std::sync::Arc;
use tracing::error;

use crate::{
    app::Command,
    messages::Messages,
    widgets::{ScrollableList, ScrollableListState},
};

use super::{watcher::WatchHandle, AppState, AppStateHandler};

const CONTACT_DISPLAY_SIZE: u16 = 100;
const MIN_LIST_DISPLAY_SIZE: u16 = 20;

#[derive(Clone)]
pub enum FlatContact {
    Group { name: String },
    Item(ContactItemType),
}

impl From<FlatContact> for ListItem<'_> {
    fn from(value: FlatContact) -> Self {
        match value {
            FlatContact::Group { name } => {
                ListItem::new(name).style(Style::new().on_dark_gray().bold())
            }
            FlatContact::Item(ContactItemType::Group(group)) => {
                ListItem::new(Text::from(group.name))
            }
            FlatContact::Item(ContactItemType::Contact(contact)) => {
                ListItem::new(Text::from(contact.name))
            }
        }
    }
}

pub enum Message {
    Init,
    LoadContacts(Option<WatchHandle>, Vec<FlatContact>),
    OpenContactPopup,
}

#[derive(Default)]
struct OpenedContact {
    contact: Option<ContactItemType>,
}

impl OpenedContact {
    fn some(contact: ContactItemType) -> Self {
        Self {
            contact: Some(contact),
        }
    }
    fn none() -> Self {
        Self::default()
    }
    fn is_open(&self) -> bool {
        self.contact.is_some()
    }
    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Option<Rect> {
        let (list_area, box_area, contact_area) =
            if area.width <= CONTACT_DISPLAY_SIZE + MIN_LIST_DISPLAY_SIZE {
                (None, Rect::default(), area)
            } else {
                let [list_area, box_area, contact_area] = Layout::horizontal([
                    Constraint::Percentage(100),
                    Constraint::Length(1),
                    Constraint::Length(CONTACT_DISPLAY_SIZE),
                ])
                .areas(area);
                (Some(list_area), box_area, contact_area)
            };

        match &mut self.contact {
            None => return Some(area),
            Some(state) => {
                frame.render_widget(Block::new().borders(Borders::LEFT), box_area);
                Self::draw_contact(frame, contact_area, state);
            }
        }

        list_area
    }

    fn draw_contact(frame: &mut Frame, area: Rect, contact: &mut ContactItemType) {
        let rows = match contact {
            ContactItemType::Contact(contact_item) => vec![
                Row::new([Cell::from("Name:"), Cell::from(contact_item.name.as_str())]).bold(),
                Row::new([
                    Cell::from("Emails:"),
                    Cell::from(
                        contact_item
                            .emails
                            .iter()
                            .map(|email| email.email.as_str())
                            .join(", "),
                    ),
                ]),
            ],
            ContactItemType::Group(contact_group_item) => vec![
                Row::new([
                    Cell::from("Name:"),
                    Cell::from(contact_group_item.name.as_str()),
                ])
                .bold(),
                Row::new([
                    Cell::from("Members:"),
                    Cell::from(
                        contact_group_item
                            .contacts
                            .iter()
                            .map(|contact| contact.name.as_str())
                            .join(", "),
                    ),
                ]),
            ],
        };

        let widths = [Constraint::Length(10), Constraint::Fill(1)];
        let table = Table::new(rows, widths).column_spacing(1);
        frame.render_widget(table, area);
    }
}

pub struct Model {
    ctx: Arc<MailUserContext>,
    contacts: Vec<FlatContact>,
    open_contact: OpenedContact,
    list_state: ScrollableListState,
    watcher: Option<WatchHandle>,
}

impl Model {
    pub fn new(ctx: Arc<MailUserContext>) -> Self {
        Self {
            ctx,
            contacts: Vec::default(),
            list_state: ScrollableListState::new(None),
            open_contact: OpenedContact::default(),
            watcher: None,
        }
    }

    fn selected_contact_item(&self) -> Option<&ContactItemType> {
        let index = self.list_state.selected()?;
        let contact = self.contacts.get(index)?;
        let FlatContact::Item(contact_item) = contact else {
            return None;
        };

        Some(contact_item)
    }

    async fn load_contacts(tether: &Tether) -> anyhow::Result<Vec<FlatContact>> {
        let list = Contact::contact_list(tether).await?;
        Ok(Self::flatten_contacts(list))
    }

    fn flatten_contacts(contacts: Vec<GroupedContacts>) -> Vec<FlatContact> {
        contacts
            .into_iter()
            .flat_map(|contact| {
                std::iter::once(FlatContact::Group {
                    name: contact.grouped_by,
                })
                .chain(contact.items.into_iter().map(FlatContact::Item))
            })
            .collect()
    }

    async fn init(ctx: Arc<MailUserContext>) -> anyhow::Result<Command<Messages>> {
        let stash = ctx.user_stash();
        let (
            list,
            WatcherHandle {
                handle, receiver, ..
            },
        ) = Contact::watch_contact_list(stash).await?;
        let (watcher, background_command) =
            WatchHandle::new_dampened(receiver, handle, move || {
                let tether = ctx.user_stash().connection();
                async move {
                    Some(match Self::load_contacts(&tether).await {
                        Ok(list) => Message::LoadContacts(None, list).into(),
                        Err(e) => {
                            let e = anyhow::anyhow!("Contact list query error: {e}");
                            error!("{e}");
                            e.into()
                        }
                    })
                }
                .boxed()
            });

        let command = Command::batch([
            Command::Message(Messages::DismissBackgroundProgress),
            Command::message(
                Message::LoadContacts(Some(watcher), Self::flatten_contacts(list)).into(),
            ),
            background_command,
        ]);
        Ok(command)
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
            KeyCode::Char('k') | KeyCode::Up => {
                self.list_state.prev();
                Command::None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.list_state.next();
                Command::None
            }
            KeyCode::Enter => Command::message(Message::OpenContactPopup.into()),
            KeyCode::Esc => {
                if self.open_contact.is_open() {
                    self.open_contact = OpenedContact::none();
                    Command::none()
                } else {
                    let ctx = self.ctx.clone();
                    Command::batch([
                        Command::message(Messages::DisplayBackgroundProgress(
                            "Loading mailbox ...".to_owned(),
                        )),
                        Command::task(async move {
                            let model = crate::app_model::mailbox::Model::new(ctx).await;
                            let message = match model {
                                Ok(model) => Messages::SwitchAppState(model.into()),
                                Err(e) => e.into(),
                            };
                            Command::batch([
                                Command::Message(Messages::DismissBackgroundProgress),
                                Command::message(message),
                            ])
                        }),
                    ])
                }
            }
            _ => Command::None,
        }
    }

    fn update(
        &mut self,
        _ctx: &Arc<proton_mail_common::MailContext>,
        message: Messages,
    ) -> Command<Messages> {
        let Messages::Contacts(message) = message else {
            return Command::None;
        };

        let ctx = self.ctx.clone();
        match message {
            Message::Init => Command::batch([
                Command::message(Messages::DisplayBackgroundProgress(
                    "Loading contacts...".to_owned(),
                )),
                Command::task(async move {
                    let result = Self::init(ctx).await;
                    result.inspect_err(|e| error!("{e}")).unwrap_or_else(|e| {
                        Command::batch([
                            Command::Message(Messages::DismissBackgroundProgress),
                            Command::message(Messages::DisplayError(None, e)),
                        ])
                    })
                }),
            ]),
            Message::LoadContacts(watcher, contacts) => {
                self.contacts = contacts;
                if let Some(watcher) = watcher {
                    self.watcher = Some(watcher);
                }
                self.list_state.set_len(self.contacts.len());
                self.list_state.select(Some(0));
                Command::none()
            }
            Message::OpenContactPopup => {
                let Some(item) = self.selected_contact_item() else {
                    return Command::none();
                };

                self.open_contact = OpenedContact::some(item.clone());
                Command::none()
            }
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let table_area = self.open_contact.draw(frame, area);

        if let Some(table_area) = table_area {
            let area = table_area.inner(Margin {
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

            let list_contacts = self
                .contacts
                .clone()
                .into_iter()
                .map_into::<ListItem<'_>>()
                .collect::<Vec<_>>();

            frame.render_stateful_widget(
                ScrollableList::new(
                    List::new(list_contacts)
                        .block(Block::new().title("Contacts").borders(Borders::all())),
                ),
                area,
                &mut self.list_state,
            );
        }
    }

    fn view_help_bar(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            Line::from(vec![
                Span::from(" ▲: ").bold(),
                Span::from("Up"),
                Span::from(" ▼: ").bold(),
                Span::from("Down"),
                Span::from(" Enter: ").bold(),
                Span::from("Open"),
                Span::from(" Esc: ").bold(),
                Span::from("Close"),
            ]),
            area,
        );
    }

    fn view_status_bar(&mut self, _frame: &mut Frame, _area: Rect) {}
}

impl From<Model> for AppState {
    fn from(value: Model) -> Self {
        Self::Contacts(value)
    }
}

impl From<Message> for Messages {
    fn from(value: Message) -> Self {
        Self::Contacts(value)
    }
}
