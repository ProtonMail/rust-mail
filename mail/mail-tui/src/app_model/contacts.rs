use crossterm::event::{Event, KeyCode};
use futures::FutureExt;
use itertools::Itertools;
use proton_core_common::{
    datatypes::{ContactGroupItem, ContactItem, ContactItemType, GroupedContacts, LocalContactId},
    models::{Contact, ContactDetailCard, ContactDetails, ContactListWatcher},
};
use proton_mail_common::MailUserContext;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin},
    prelude::Rect,
    style::{Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Cell, List, ListItem, Row, Table},
};
use stash::stash::{Tether, WatcherHandle};
use std::fmt::Write as _;
use std::sync::Arc;
use tracing::error;

use crate::{
    app::Command,
    messages::Messages,
    widgets::{ScrollableList, ScrollableListState},
};

use super::{AppState, AppStateHandler, watcher::WatchHandle};

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
    LoadContacts(Vec<FlatContact>),
    LoadContactDetails(ContactDetails),
    OpenContactPopup,
}

#[derive(Default)]
enum OpenedContactState {
    #[default]
    None,
    Loading(ContactItem),
    Contact(ContactDetails),
    Group(ContactGroupItem),
}

impl OpenedContactState {
    fn is_open(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// Returns the area to be used for the list
    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Option<Rect> {
        let (list_area, _border_area, contact_area) =
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

        match self {
            OpenedContactState::None => return Some(area),
            OpenedContactState::Loading(contact_item) => {
                Self::draw_contact_item(frame, contact_area, contact_item);
            }
            OpenedContactState::Contact(items) => {
                Self::draw_contact_details(frame, contact_area, items);
            }
            OpenedContactState::Group(group) => {
                Self::draw_group(frame, contact_area, group);
            }
        }

        list_area
    }

    fn draw_contact_item(frame: &mut Frame, area: Rect, contact: &ContactItem) {
        let rows = [
            Row::new([Cell::from("Name:"), Cell::from(contact.name.as_str())]).bold(),
            Row::new([
                Cell::from("Emails:"),
                Cell::from(
                    contact
                        .emails
                        .iter()
                        .map(|email| email.email.as_str())
                        .join(", "),
                ),
            ]),
        ];

        let widths = [Constraint::Length(20), Constraint::Fill(1)];
        let table = Table::new(rows, widths).column_spacing(1);
        frame.render_widget(table, area);
    }

    #[allow(
        clippy::too_many_lines,
        reason = "It's a straightforward renedering function with no logic and no further fn calls"
    )]
    fn draw_contact_details(frame: &mut Frame, area: Rect, contacts: &ContactDetails) {
        let mut rows = vec![];
        rows.push(Row::new([Cell::from("Name:"), Cell::from(&*contacts.item.name)]).bold());
        rows.push(Row::new([
            Cell::from("Emails:"),
            Cell::from(
                contacts
                    .item
                    .emails
                    .iter()
                    .map(|email| &*email.email)
                    .join(", "),
            ),
        ]));

        for ContactDetailCard {
            extended_name,
            address,
            phones,
            birthday,
            anniversary,
            urls,
            gender,
            notes,
            photos,
            logos,
            titles,
            roles,
            languages,
            timezones,
            members: member,
            organizations,
        } in &contacts.cards
        {
            if let Some(proton_core_common::models::ExtendedName {
                last,
                first,
                additional,
                prefix,
                suffix,
            }) = extended_name
            {
                let mut extended_name_repr = String::new();
                if let Some(prefix) = prefix {
                    write!(&mut extended_name_repr, "{prefix} ").unwrap();
                }
                if let Some(first) = first {
                    write!(&mut extended_name_repr, "{first} ").unwrap();
                }
                if let Some(last) = last {
                    write!(&mut extended_name_repr, "{last} ").unwrap();
                }
                if let Some(suffix) = suffix {
                    write!(&mut extended_name_repr, "{suffix}").unwrap();
                }
                if let Some(additional) = additional {
                    write!(&mut extended_name_repr, " {additional}").unwrap();
                }

                let extended_name = extended_name_repr.trim().to_string();
                if !extended_name.is_empty() {
                    rows.push(Row::new([
                        Cell::from("Extended Name: "),
                        Cell::from(extended_name),
                    ]));
                }
            }

            for address in address {
                let addr_type = address.addr_type.iter().map(ToString::to_string).join(", ");
                rows.push(Row::new([Cell::from(
                    format!("Address {addr_type}").bold(),
                )]));
                if !address.street.is_empty() {
                    rows.push(Row::new([
                        Cell::from("Street:"),
                        Cell::from(&*address.street),
                    ]));
                }
                if !address.city.is_empty() {
                    rows.push(Row::new([Cell::from("City:"), Cell::from(&*address.city)]));
                }
                if !address.region.is_empty() {
                    rows.push(Row::new([
                        Cell::from("Region:"),
                        Cell::from(&*address.region),
                    ]));
                }
                if !address.postal_code.is_empty() {
                    rows.push(Row::new([
                        Cell::from("Postal Code:"),
                        Cell::from(&*address.postal_code),
                    ]));
                }
                if !address.country.is_empty() {
                    rows.push(Row::new([
                        Cell::from("Country:"),
                        Cell::from(&*address.country),
                    ]));
                }
            }
            for phone in phones {
                rows.push(Row::new([Cell::from("Phone:"), Cell::from(&*phone.number)]));
            }
            if let Some(birthday) = birthday {
                rows.push(Row::new([
                    Cell::from("Birthday:"),
                    Cell::from(birthday.to_string()),
                ]));
            }

            if let Some(anniversary) = anniversary {
                rows.push(Row::new([
                    Cell::from("Anniversary:"),
                    Cell::from(anniversary.to_string()),
                ]));
            }

            for url in urls {
                rows.push(Row::new([Cell::from("Url:"), Cell::from(&*url.url)]));
            }

            for note in notes {
                // FIXME: This might not fit!
                rows.push(Row::new([Cell::from("Note:"), Cell::from(&**note)]));
            }

            if let Some(gender) = gender {
                rows.push(Row::new([
                    Cell::from("Gender: "),
                    Cell::from(gender.to_string()),
                ]));
            }

            if !photos.is_empty() {
                rows.push(Row::new([
                    Cell::from("Photos:"),
                    Cell::from(photos.len().to_string()),
                ]));
            }

            if !logos.is_empty() {
                rows.push(Row::new([
                    Cell::from("Logos:"),
                    Cell::from(logos.len().to_string()),
                ]));
            }

            for title in titles {
                rows.push(Row::new([Cell::from("Title:"), Cell::from(title.as_str())]));
            }

            for role in roles {
                rows.push(Row::new([Cell::from("Role:"), Cell::from(role.as_str())]));
            }

            for language in languages {
                rows.push(Row::new([
                    Cell::from("Language:"),
                    Cell::from(language.as_str()),
                ]));
            }

            for timezone in timezones {
                rows.push(Row::new([
                    Cell::from("Timezone:"),
                    Cell::from(timezone.as_str()),
                ]));
            }

            for member_entry in member {
                rows.push(Row::new([
                    Cell::from("Member:"),
                    Cell::from(member_entry.as_str()),
                ]));
            }

            for org in organizations {
                rows.push(Row::new([
                    Cell::from("Organization:"),
                    Cell::from(org.as_str()),
                ]));
            }
        }

        let widths = [Constraint::Length(10), Constraint::Fill(1)];
        let table = Table::new(rows, widths).column_spacing(1);
        frame.render_widget(table, area);
    }

    fn draw_group(frame: &mut Frame, area: Rect, group: &ContactGroupItem) {
        let rows = [
            Row::new([Cell::from("Name:"), Cell::from(group.name.as_str())]).bold(),
            Row::new([
                Cell::from("Members:"),
                Cell::from(
                    group
                        .contacts
                        .iter()
                        .map(|contact| contact.name.as_str())
                        .join(", "),
                ),
            ]),
        ];

        let widths = [Constraint::Length(10), Constraint::Fill(1)];
        let table = Table::new(rows, widths).column_spacing(1);
        frame.render_widget(table, area);
    }
}

pub struct Model {
    ctx: Arc<MailUserContext>,
    contacts: Vec<FlatContact>,
    open_contact: OpenedContactState,
    list_state: ScrollableListState,
    watcher: Option<WatchHandle>,
}

impl Model {
    pub fn new(ctx: Arc<MailUserContext>) -> Self {
        Self {
            ctx,
            contacts: Vec::default(),
            list_state: ScrollableListState::new(None),
            open_contact: OpenedContactState::default(),
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

    fn load_contact_details(&self, contact_id: LocalContactId) -> Command<Messages> {
        let ctx = self.ctx.clone();
        Command::task(async move {
            let ctx = ctx.user_context();
            match ContactDetails::get_from_contact(ctx, contact_id).await {
                Ok(details) => Command::Message(Message::LoadContactDetails(details).into()),
                Err(e) => {
                    tracing::error!("{e:?}");
                    Command::message(e.into())
                }
            }
        })
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

    /// Initializes model
    fn init(&mut self) -> Command<Messages> {
        let ctx = self.ctx.clone();
        let (watcher, background_command) = match Self::init_watch(ctx) {
            Ok(t) => t,
            Err(e) => {
                error!("{e:?}");
                return Command::batch([
                    Command::Message(Messages::DismissBackgroundProgress),
                    Command::message(Messages::DisplayError(None, e)),
                ]);
            }
        };

        self.watcher = Some(watcher);
        let ctx = self.ctx.clone();
        Command::task(async move {
            (Self::init_contact_list(ctx, background_command).await)
                .inspect_err(|e| error!("{e:?}"))
                .unwrap_or_else(|e| {
                    Command::batch([
                        Command::Message(Messages::DismissBackgroundProgress),
                        Command::message(Messages::DisplayError(None, e)),
                    ])
                })
        })
    }

    /// Initializes the Watcher over contact lists
    ///
    /// # Errors
    ///
    /// Might result an error if it was unable to subscribe to the database
    ///
    fn init_watch(ctx: Arc<MailUserContext>) -> anyhow::Result<(WatchHandle, Command<Messages>)> {
        let stash = ctx.user_stash();
        let WatcherHandle {
            handle, receiver, ..
        } = stash.subscribe_to(|sender| Box::new(ContactListWatcher::new(sender)))?;
        let (watcher, background_command) =
            WatchHandle::new_dampened(receiver, handle, move || {
                let tether = ctx.user_stash().connection();
                async move {
                    Some(match Self::load_contacts(&tether).await {
                        Ok(list) => Message::LoadContacts(list).into(),
                        Err(e) => {
                            let e = anyhow::anyhow!("Contact list query error: {e}");
                            error!("{e:?}");
                            e.into()
                        }
                    })
                }
                .boxed()
            });

        Ok((watcher, background_command))
    }

    /// Initializes contact list by fetching it from Database
    async fn init_contact_list(
        ctx: Arc<MailUserContext>,
        background_command: Command<Messages>,
    ) -> anyhow::Result<Command<Messages>> {
        let tether = ctx.user_stash().connection();
        let list = Self::load_contacts(&tether).await?;
        Ok(Command::batch([
            Command::Message(Messages::DismissBackgroundProgress),
            Command::message(Message::LoadContacts(list).into()),
            background_command,
        ]))
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
                    self.open_contact = OpenedContactState::None;
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

        match message {
            Message::Init => Command::batch([
                Command::message(Messages::DisplayBackgroundProgress(
                    "Loading contacts...".to_owned(),
                )),
                self.init(),
            ]),
            Message::LoadContacts(contacts) => {
                self.contacts = contacts;
                self.list_state.set_len(self.contacts.len());
                self.list_state.select(Some(0));
                Command::none()
            }
            Message::OpenContactPopup => {
                match self.selected_contact_item().cloned() {
                    // For contacts we load the details
                    Some(ContactItemType::Contact(contact)) => {
                        let id = contact.local_id;
                        self.open_contact = OpenedContactState::Loading(contact);
                        self.load_contact_details(id)
                    }
                    // For groups for now we use the available data (to be changed in the future)
                    Some(ContactItemType::Group(group)) => {
                        self.open_contact = OpenedContactState::Group(group);
                        Command::None
                    }
                    None => Command::none(),
                }
            }
            Message::LoadContactDetails(contacts) => {
                self.open_contact = OpenedContactState::Contact(contacts);
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
