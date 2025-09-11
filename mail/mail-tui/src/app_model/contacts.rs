use anyhow::anyhow;
use crossterm::event::{Event, KeyCode, KeyModifiers};
use futures::FutureExt;
use itertools::Itertools;
use proton_core_common::{
    datatypes::{
        ContactGroupItem, ContactItem, ContactItemType, GroupedContacts, LocalContactId,
        contact_details::{
            ContactDetailAddress, ContactDetailsEmail, ContactField, ExtendedName,
            InspectableContactDetails, Telephone, VCardUrl,
        },
    },
    models::{Contact, ContactListWatcher},
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
use stash::stash::Tether;
use std::fmt::Write as _;
use std::sync::Arc;
use tracing::error;

use crate::{
    app::Command,
    app_model::mailbox::{poll_event_loop, refresh},
    messages::Messages,
    widgets::{ScrollableList, ScrollableListState, utils::ScrollableState},
};

use super::{AppState, AppStateHandler, watcher::TuiWatchHandle};

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
    LoadContactDetails(InspectableContactDetails),
    OpenContactPopup,
}

#[derive(Default)]
enum OpenedContactState {
    #[default]
    None,
    Loading(ContactItem),
    Contact(InspectableContactDetails),
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
            OpenedContactState::Loading(item) => {
                Self::draw_contact_item(frame, contact_area, item);
            }
            OpenedContactState::Contact(details) => {
                Self::draw_contact_details(frame, contact_area, details);
            }
            OpenedContactState::Group(group) => {
                Self::draw_group(frame, contact_area, group);
            }
        }

        list_area
    }

    fn draw_contact_item(frame: &mut Frame, area: Rect, item: &ContactItem) {
        let mut rows = vec![];

        let mut title_cell_size = 0;
        let mut add_row = |title: &str, body: &str| {
            title_cell_size = title_cell_size.max(title.len());
            if !body.is_empty() {
                rows.push(Row::new([
                    Cell::from(title.to_string()),
                    Cell::from(body.to_string()),
                ]));
            }
        };

        add_row("Name:", &item.name);
        for email in &item.emails {
            add_row("Email:", &email.email);
        }

        let widths = [
            Constraint::Length(TryInto::<u16>::try_into(title_cell_size).unwrap()),
            Constraint::Fill(1),
        ];
        let table = Table::new(rows, widths).column_spacing(1);
        frame.render_widget(table, area);
    }

    #[allow(
        clippy::too_many_lines,
        reason = "It's a straightforward renedering function with no logic and no further fn calls"
    )]
    fn draw_contact_details(frame: &mut Frame, area: Rect, details: &InspectableContactDetails) {
        let mut rows = vec![];

        let mut title_cell_size = 0;
        let mut add_row = |title: &str, body: &str| {
            title_cell_size = title_cell_size.max(title.len());
            let body = body.trim();
            if !body.is_empty() {
                rows.push(Row::new([
                    Cell::from(title.trim().to_string()),
                    Cell::from(body.trim().to_string()),
                ]));
            }
        };

        {
            let ExtendedName {
                last,
                first,
                additional,
                prefix,
                suffix,
            } = &details.extended_name;
            let mut extended_name_repr = String::new();
            if let Some(prefix) = prefix {
                write!(&mut extended_name_repr, "{prefix} ").unwrap();
            }
            if let Some(additional) = additional {
                write!(&mut extended_name_repr, " {additional}").unwrap();
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

            add_row("Extended Name:", &extended_name_repr);
        }

        for field in &details.fields {
            match field {
                ContactField::Anniversary(date) => {
                    add_row("Anniversary:", &date.to_string());
                }
                ContactField::Birthday(date) => {
                    add_row("Birthday:", &date.to_string());
                }
                ContactField::Address(items) => {
                    for ContactDetailAddress {
                        street,
                        city,
                        region,
                        postal_code,
                        country,
                        addr_type,
                    } in items
                    {
                        let addr_type = addr_type.iter().map(ToString::to_string).join(", ");
                        add_row("Address:", &addr_type);

                        add_row("Street:", street.as_deref().unwrap_or_default());
                        add_row("City:", city.as_deref().unwrap_or_default());
                        add_row("Region:", region.as_deref().unwrap_or_default());
                        add_row("Postal Code:", postal_code.as_deref().unwrap_or_default());
                        add_row("Country:", country.as_deref().unwrap_or_default());
                    }
                }
                ContactField::Emails(items) => {
                    for ContactDetailsEmail {
                        email_type,
                        email,
                        groups,
                    } in items
                    {
                        let types_str = email_type.iter().map(ToString::to_string).join(", ");
                        let groups_str = if groups.is_empty() {
                            String::new()
                        } else {
                            let group_names =
                                groups.iter().map(|group| group.name.as_str()).join(", ");
                            format!(" [groups: {group_names}]")
                        };

                        let text = format!("{types_str} {email}{groups_str}");
                        add_row("Email:", &text);
                    }
                }
                ContactField::Phones(items) => {
                    for Telephone { number, tel_types } in items {
                        let text = format!(
                            "{} {number}",
                            tel_types.iter().map(ToString::to_string).join(", ")
                        );
                        add_row("Telephone:", &text);
                    }
                }
                ContactField::Gender(item) => {
                    add_row("Gender:", &item.to_string());
                }
                ContactField::Languages(items) => {
                    for item in items {
                        add_row("Language:", item);
                    }
                }
                ContactField::Members(items) => {
                    for item in items {
                        add_row("Member:", item);
                    }
                }
                ContactField::Notes(items) => {
                    // FIXME: This might not fit!
                    for item in items {
                        add_row("Note:", item);
                    }
                }
                ContactField::Organizations(items) => {
                    for item in items {
                        add_row("Organizationn:", item);
                    }
                }
                ContactField::Roles(items) => {
                    for item in items {
                        add_row("Role:", item);
                    }
                }
                ContactField::TimeZones(items) => {
                    for item in items {
                        add_row("Timezone:", item);
                    }
                }
                ContactField::Titles(items) => {
                    for item in items {
                        add_row("Title:", item);
                    }
                }
                ContactField::Urls(items) => {
                    for VCardUrl { url, url_type } in items {
                        let text = format!(
                            "{} {url}",
                            url_type.iter().map(ToString::to_string).join(", ")
                        );
                        add_row("Url:", &text);
                    }
                }
                // TODO: Do something with these, link, term image...
                ContactField::Photos(_) | ContactField::Logos(_) => (),
            }
        }

        let widths = [
            Constraint::Length(TryInto::<u16>::try_into(title_cell_size).unwrap()),
            Constraint::Fill(1),
        ];
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

pub struct ContactsModel {
    ctx: Arc<MailUserContext>,
    contacts: Vec<FlatContact>,
    open_contact: OpenedContactState,
    list_state: ScrollableListState,
    watcher: Option<TuiWatchHandle>,
}

impl ContactsModel {
    pub fn new(ctx: Arc<MailUserContext>) -> Self {
        Self {
            ctx,
            contacts: Vec::default(),
            list_state: ScrollableListState::new(None),
            open_contact: OpenedContactState::default(),
            watcher: None,
        }
    }

    pub fn ctx(&self) -> Arc<MailUserContext> {
        Arc::clone(&self.ctx)
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
            match async {
                let mut tether = ctx.stash().connection().await?;
                InspectableContactDetails::get_from_contact(ctx, contact_id, &mut tether).await
            }
            .await
            {
                Ok(details) => Command::Message(Message::LoadContactDetails(details).into()),
                Err(e) => {
                    tracing::error!("{e:?}");
                    Command::message(e)
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
    fn init_watch(
        ctx: Arc<MailUserContext>,
    ) -> anyhow::Result<(TuiWatchHandle, Command<Messages>)> {
        let stash = ctx.user_stash();
        let handle = stash.subscribe_to(|sender| Box::new(ContactListWatcher::new(sender)))?;
        let (watcher, background_command) =
            TuiWatchHandle::from_watcher_handle(handle, move || {
                let ctx = ctx.clone();
                async move {
                    let Ok(tether) = ctx.user_stash().connection().await else {
                        return Some(Messages::DisplayError(
                            None,
                            anyhow!("Failed to acquire db connection"),
                        ));
                    };
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
        let tether = ctx.user_stash().connection().await?;
        let list = Self::load_contacts(&tether).await?;
        Ok(Command::batch([
            Command::Message(Messages::DismissBackgroundProgress),
            Command::message(Message::LoadContacts(list)),
            background_command,
        ]))
    }
}

impl AppStateHandler for ContactsModel {
    fn on_state_enter(&mut self) -> Command<Messages> {
        Command::message(Message::Init)
    }
    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        let Event::Key(key) = event else {
            return Command::None;
        };
        if self.list_state.handle_event(key.code) {
            return Command::None;
        }

        match key.code {
            KeyCode::Enter => Command::message(Message::OpenContactPopup),
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
                            let model = crate::app_model::mailbox::MailboxModel::new(ctx).await;
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
            KeyCode::F(5) if key.modifiers.contains(KeyModifiers::SHIFT) => {
                refresh(self.ctx.as_arc())
            }
            KeyCode::F(5) => poll_event_loop(self.ctx.as_arc()),
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
                        let group_clone = group.clone();
                        self.open_contact = OpenedContactState::Group(group);

                        // Sanity check that our data is legit, only in debug mode.
                        if cfg!(debug_assertions) {
                            let ctx = self.ctx.clone();
                            Command::task(async move {
                                let tether = ctx.user_stash().connection().await.unwrap();
                                let group_from_db =
                                    Contact::contact_group_by_id(&tether, group_clone.local_id)
                                        .await
                                        .unwrap();
                                assert_eq!(group_from_db, group_clone);
                                Command::None
                            })
                        } else {
                            Command::None
                        }
                    }
                    None => Command::none(),
                }
            }
            Message::LoadContactDetails(contacts) => {
                if let OpenedContactState::Loading(_) = self.open_contact {
                    self.open_contact = OpenedContactState::Contact(contacts);
                }
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

    fn help_options(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("k, ▲", "Go up"),
            ("j, ▼", "Go down"),
            ("enter", "See details for a contact"),
            ("esc", "Close the contact"),
            ("Shift+F5", "Reload all data from server"),
            ("F5", "Refresh (Force event loop poll)"),
        ]
    }

    fn view_status_bar(&mut self, _frame: &mut Frame, _area: Rect) {}
}

impl From<ContactsModel> for AppState {
    fn from(value: ContactsModel) -> Self {
        Self::Contacts(value)
    }
}

impl From<Message> for Messages {
    fn from(value: Message) -> Self {
        Self::Contacts(value)
    }
}
