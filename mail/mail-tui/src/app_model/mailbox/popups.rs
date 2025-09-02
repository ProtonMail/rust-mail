use crate::app::Command;
use crate::app_model::mailbox::{ConversationMessage, Items, Message, MessageMessage};
use crate::messages::Messages;
use crate::widgets::utils::ScrollableState;
use crate::widgets::{AsList, ScrollableList, ScrollableListState};
use proton_core_common::datatypes::{LabelType, LocalLabelId};
use proton_core_common::models::Label;
use proton_mail_common::actions::LabelAsAction;
use proton_mail_common::datatypes::ViewMode;
use proton_mail_common::models::{Conversation, LabelWithCounters, MailLabel};
use proton_mail_common::{MailContextResult, MailUserContext, Sidebar};
use ratatui::crossterm::event::{Event, KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Tabs};
use ratatui::{Frame, symbols};
use stash::orm::Model;
use std::sync::Arc;

use super::LabelAs;

pub struct MoveItemPopup {
    folders: Vec<Label>,
    list_state: ScrollableListState,
    item: Items,
}

impl MoveItemPopup {
    pub async fn new(ctx: &MailUserContext, item: Items) -> MailContextResult<Self> {
        //TODO: improve
        let tether = ctx.user_stash().connection().await?;
        let mut folders = Label::find_by_kind(LabelType::Folder, &tether).await?;
        folders.retain(MailLabel::is_movable_folder);
        let mut system = Label::find_by_kind(LabelType::System, &tether).await?;
        system.retain(MailLabel::is_movable_folder);
        folders.extend(system);
        Ok(Self {
            folders,
            item,
            list_state: ScrollableListState::new(Some(0)),
        })
    }
    fn selected_label_id(&self) -> Option<LocalLabelId> {
        let index = self.list_state.selected()?;
        self.folders.get(index).map(Model::id)
    }
}

impl crate::app_model::Popup for MoveItemPopup {
    fn title(&self) -> Option<String> {
        Some("Select Folder to Move to".to_owned())
    }

    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        let Event::Key(key) = event else {
            return Command::None;
        };
        if self.list_state.handle_event(key.code) {
            return Command::None;
        }

        match key.code {
            KeyCode::Enter => self
                .selected_label_id()
                .map(|id| match self.item.clone() {
                    Items::Conversation(item_id) => Command::batch([
                        Command::message(Messages::DismissPopup),
                        Command::message(ConversationMessage::MoveTo(item_id, id)),
                    ]),
                    Items::Message(item_id) => Command::batch([
                        Command::message(Messages::DismissPopup),
                        Command::message(MessageMessage::MoveTo(item_id, id)),
                    ]),
                })
                .unwrap_or_default(),
            _ => Command::None,
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let list = self.folders.as_list();
        let list = ScrollableList::new(list);
        frame.render_stateful_widget(list, area, &mut self.list_state);
    }
}

pub struct LabelItemPopup {
    labels: Vec<LabelAsAction>,
    list_state: ScrollableListState,
    item: Items,
    must_archive: bool,
}

impl LabelItemPopup {
    pub async fn new(ctx: &MailUserContext, item: Items) -> MailContextResult<Self> {
        let stash = ctx.user_stash();
        let tether = stash.connection().await?;
        let labels = match item.clone() {
            Items::Conversation(local_ids) => {
                Conversation::available_label_as_actions(local_ids, &tether).await?
            }
            Items::Message(local_ids) => {
                proton_mail_common::models::Message::available_label_as_actions(local_ids, &tether)
                    .await?
            }
        };

        Ok(Self {
            labels,
            item,
            list_state: ScrollableListState::new(Some(0)),
            must_archive: false,
        })
    }
    fn selected_label(&self) -> Option<&LabelAsAction> {
        let index = self.list_state.selected()?;
        Some(
            self.labels
                .get(index)
                .expect("Index for labels out of bounds"),
        )
    }

    fn selected_label_mut(&mut self) -> Option<&mut LabelAsAction> {
        let index = self.list_state.selected()?;
        Some(
            self.labels
                .get_mut(index)
                .expect("Index for labels out of bounds"),
        )
    }
}
impl crate::app_model::Popup for LabelItemPopup {
    fn title(&self) -> Option<String> {
        Some("Select or deselect labels".to_owned())
    }

    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        let Event::Key(key) = event else {
            return Command::None;
        };
        if self.list_state.handle_event(key.code) {
            return Command::None;
        }

        match key.code {
            KeyCode::Char('s') => {
                if let Some(label) = self.selected_label_mut() {
                    label.is_selected = match label.is_selected {
                        // If it's partially selected or if it's selected: Deselect it
                        Some(true) | None => Some(false),
                        // If it's not selected: Select it
                        Some(false) => Some(true),
                    };
                }
                Command::None
            }
            KeyCode::Char(' ') => {
                self.must_archive = !self.must_archive;
                Command::None
            }
            KeyCode::Enter => {
                let Some(action) = self.selected_label() else {
                    return Command::None;
                };
                let mut selected_label_ids = vec![];
                let mut partially_selected_label_ids = vec![];

                for label in &self.labels {
                    match label.is_selected {
                        Some(true) => selected_label_ids.push(label.label_id),
                        None => partially_selected_label_ids.push(label.label_id),
                        Some(false) => (),
                    }
                }
                match self.item.clone() {
                    Items::Conversation(item_ids) => {
                        let label_as = Box::new(LabelAs {
                            source_label_id: action.label_id,
                            item_ids,
                            selected_label_ids,
                            partially_selected_label_ids,
                            must_archive: self.must_archive,
                        });
                        Command::batch([
                            Command::message(Messages::DismissPopup),
                            Command::message(ConversationMessage::LabelAs(label_as)),
                        ])
                    }
                    Items::Message(item_ids) => {
                        let label_as = Box::new(LabelAs {
                            source_label_id: action.label_id,
                            item_ids,
                            selected_label_ids,
                            partially_selected_label_ids,
                            must_archive: self.must_archive,
                        });
                        Command::batch([
                            Command::message(Messages::DismissPopup),
                            Command::message(MessageMessage::LabelAs(label_as)),
                        ])
                    }
                }
            }
            _ => Command::None,
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let [list_area, must_archive_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);

        let list = self
            .labels
            .iter()
            .map(|x| {
                let sigl = match x.is_selected {
                    Some(true) => "✔",
                    Some(false) => " ",
                    None => "~",
                };
                Span::from(format!("{sigl} {}", x.name))
            }) // TODO: Color this
            .collect::<List<'_>>();
        let list = ScrollableList::new(list);
        frame.render_stateful_widget(list, list_area, &mut self.list_state);
        let x = Line::from(format!(
            "[spacebar] Also archive: [{}]",
            if self.must_archive { 'x' } else { ' ' }
        ));

        frame.render_widget(x, must_archive_area);
    }
}

pub struct LabelSelectPopup {
    system: Vec<LabelWithCounters>,
    folders: Vec<LabelWithCounters>,
    labels: Vec<LabelWithCounters>,
    system_list_state: ScrollableListState,
    folder_list_state: ScrollableListState,
    labels_list_state: ScrollableListState,
    active_label: LabelType,
    view_mode: ViewMode,
}

impl LabelSelectPopup {
    pub async fn new(
        ctx: Arc<MailUserContext>,
        current_label: &LabelWithCounters,
        view_mode: ViewMode,
    ) -> anyhow::Result<Self> {
        let tether = ctx.user_stash().connection().await?;
        let sidebar = Sidebar;
        let system = sidebar.system_labels(&tether).await?;
        let labels = sidebar.custom_labels(&tether).await?;
        let folders = sidebar.custom_folders(&tether).await?;

        let system =
            LabelWithCounters::from_ids(&tether, system.iter().map(|x| x.local_id)).await?;
        let labels =
            LabelWithCounters::from_ids(&tether, labels.iter().map(|x| x.local_id)).await?;
        let folders =
            LabelWithCounters::from_ids(&tether, folders.iter().map(|x| x.local_id)).await?;

        let system_index = system
            .iter()
            .position(|label| current_label.local_id.unwrap() == label.label().local_id.unwrap())
            .unwrap_or_default();
        let folder_index = folders
            .iter()
            .position(|label| current_label.local_id.unwrap() == label.label().local_id.unwrap())
            .unwrap_or_default();
        let labels_index = labels
            .iter()
            .position(|label| current_label.local_id.unwrap() == label.label().local_id.unwrap())
            .unwrap_or_default();

        Ok(Self {
            system,
            folders,
            labels,
            system_list_state: ScrollableListState::new(Some(system_index)),
            folder_list_state: ScrollableListState::new(Some(folder_index)),
            labels_list_state: ScrollableListState::new(Some(labels_index)),
            active_label: current_label.label_type,
            view_mode,
        })
    }

    fn selected_tab_index(&self) -> usize {
        match self.active_label {
            LabelType::Label => 2,
            LabelType::Folder => 1,
            LabelType::System | LabelType::ContactGroup => 0,
        }
    }

    fn selected_label_list(&mut self) -> (&[LabelWithCounters], &mut ScrollableListState) {
        match self.active_label {
            LabelType::Label => (&self.labels, &mut self.labels_list_state),
            LabelType::Folder => (&self.folders, &mut self.folder_list_state),
            LabelType::System | LabelType::ContactGroup => {
                (&self.system, &mut self.system_list_state)
            }
        }
    }

    fn switch_to_next_tab(&mut self) {
        self.active_label = match self.active_label {
            LabelType::Label => LabelType::System,
            LabelType::Folder => LabelType::Label,
            LabelType::System | LabelType::ContactGroup => LabelType::Folder,
        }
    }

    fn switch_to_prev_tab(&mut self) {
        self.active_label = match self.active_label {
            LabelType::Label => LabelType::Folder,
            LabelType::Folder => LabelType::System,
            LabelType::System | LabelType::ContactGroup => LabelType::Label,
        }
    }
}

impl crate::app_model::Popup for LabelSelectPopup {
    fn title(&self) -> Option<String> {
        Some("Select Label".to_owned())
    }

    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        let Event::Key(key) = event else {
            return Command::None;
        };
        if self.selected_label_list().1.handle_event(key.code) {
            return Command::none();
        }

        match key.code {
            KeyCode::Tab => {
                if key.modifiers.intersects(KeyModifiers::SHIFT) {
                    self.switch_to_prev_tab();
                } else {
                    self.switch_to_next_tab();
                }
                Command::None
            }
            KeyCode::Enter => {
                let (labels, list_state) = self.selected_label_list();
                let Some(index) = list_state.selected() else {
                    return Command::None;
                };
                let Some(label) = labels.get(index) else {
                    return Command::None;
                };

                Command::batch([
                    Command::message(Messages::DismissPopup),
                    Command::message(Message::SelectLabel(label.label().id())),
                ])
            }

            _ => Command::None,
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let [tab_area, list_area] =
            Layout::vertical([Constraint::Length(3), Constraint::Percentage(100)]).areas(area);

        let view_mode = self.view_mode;
        let tabs = Tabs::new(vec!["Default", "Folders", "Labels"])
            .block(Block::new().borders(Borders::ALL))
            .select(self.selected_tab_index())
            .divider(symbols::line::VERTICAL)
            .padding(" ", " ");
        frame.render_widget(tabs, tab_area);

        let (labels, list_state) = self.selected_label_list();

        let items = labels
            .iter()
            .map(|label_with_counters| {
                let label = label_with_counters.label();
                let (unread_count, total_count) = if view_mode == ViewMode::Conversations {
                    (
                        label_with_counters.unread_conv,
                        label_with_counters.total_conv,
                    )
                } else {
                    (
                        label_with_counters.unread_msg,
                        label_with_counters.total_msg,
                    )
                };
                let name = label.path.as_deref().unwrap_or(label.name.as_str());
                let text = format!("[{unread_count:04}|{total_count:04}] {name}");
                ListItem::from(text)
            })
            .collect::<Vec<_>>();

        let list = ScrollableList::new(List::new(items));
        frame.render_stateful_widget(list, list_area, list_state);
    }
}
