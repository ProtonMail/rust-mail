use crate::app::Command;
use crate::app_model::mailbox::{ConversationMessage, Item, Message};
use crate::messages::Messages;
use crate::widgets::{AsList, ScrollableList, ScrollableListState};
use anyhow::anyhow;
use crossterm::event::{Event, KeyCode, KeyModifiers};
use proton_core_common::db::DBResult;
use proton_mail_common::db::{LocalLabel, LocalLabelId, LocalLabelWithCount};
use proton_mail_common::proton_api_mail::domain::LabelType;
use proton_mail_common::{MailContextResult, MailUserContext};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, Borders, List, ListItem, Tabs};
use ratatui::{symbols, Frame};

pub struct MoveItemPopup {
    folders: Vec<LocalLabel>,
    list_state: ScrollableListState,
    item: Item,
}

impl MoveItemPopup {
    pub fn new(ctx: &MailUserContext, item: Item) -> MailContextResult<Self> {
        let folders = ctx.movable_folders()?;
        Ok(Self {
            folders,
            item,
            list_state: ScrollableListState::new(Some(0)),
        })
    }
    fn selected_label_id(&self) -> Option<LocalLabelId> {
        let index = self.list_state.selected()?;
        self.folders.get(index).map(|v| v.id)
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

        match key.code {
            KeyCode::Up => {
                self.list_state.prev();
                Command::None
            }
            KeyCode::Down => {
                self.list_state.next();
                Command::None
            }
            KeyCode::Enter => self
                .selected_label_id()
                .map(|id| match self.item {
                    Item::Conversation(item_id) => {
                        Command::message(ConversationMessage::MoveConversation(item_id, id).into())
                    }
                    Item::Message(_) => Command::message(Messages::DisplayError(
                        None,
                        anyhow!("Not Yet Implemented"),
                    )),
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
    labels: Vec<LocalLabel>,
    list_state: ScrollableListState,
    item: Item,
    apply: bool,
}

impl LabelItemPopup {
    pub fn new(ctx: &MailUserContext, item: Item, apply: bool) -> MailContextResult<Self> {
        let labels = ctx.get_labels_by_type(LabelType::Label)?;
        Ok(Self {
            labels,
            item,
            list_state: ScrollableListState::new(Some(0)),
            apply,
        })
    }
    fn selected_label_id(&self) -> Option<LocalLabelId> {
        let index = self.list_state.selected()?;
        self.labels.get(index).map(|v| v.id)
    }
}
impl crate::app_model::Popup for LabelItemPopup {
    fn title(&self) -> Option<String> {
        Some(if self.apply {
            "Select Label to Apply".to_owned()
        } else {
            "Select Label to Remove".to_owned()
        })
    }

    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        let Event::Key(key) = event else {
            return Command::None;
        };

        match key.code {
            KeyCode::Up => {
                self.list_state.prev();
                Command::None
            }
            KeyCode::Down => {
                self.list_state.next();
                Command::None
            }
            KeyCode::Enter => self
                .selected_label_id()
                .map(|id| match self.item {
                    Item::Conversation(item_id) => {
                        if self.apply {
                            Command::message(
                                ConversationMessage::LabelConversation(item_id, id).into(),
                            )
                        } else {
                            Command::message(
                                ConversationMessage::UnlabelConversation(item_id, id).into(),
                            )
                        }
                    }
                    Item::Message(_) => Command::message(Messages::DisplayError(
                        None,
                        anyhow!("Not Yet Implemented"),
                    )),
                })
                .unwrap_or_default(),
            _ => Command::None,
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let list = self.labels.as_list();
        let list = ScrollableList::new(list);
        frame.render_stateful_widget(list, area, &mut self.list_state);
    }
}

pub struct LabelSelectPopup {
    system: Vec<LocalLabelWithCount>,
    folders: Vec<LocalLabelWithCount>,
    labels: Vec<LocalLabelWithCount>,
    system_list_state: ScrollableListState,
    folder_list_state: ScrollableListState,
    labels_list_state: ScrollableListState,
    active_label: LabelType,
}

impl LabelSelectPopup {
    pub fn new(ctx: &MailUserContext, current_label: &LocalLabel) -> MailContextResult<Self> {
        let (system, folders, labels) = ctx.db_read(
            |conn| -> DBResult<(
                Vec<LocalLabelWithCount>,
                Vec<LocalLabelWithCount>,
                Vec<LocalLabelWithCount>,
            )> {
                let system = conn.label_by_type_ordered_with_message_count(LabelType::System)?;
                let folders = conn.label_by_type_ordered_with_message_count(LabelType::Folder)?;
                let labels = conn.label_by_type_ordered_with_message_count(LabelType::Label)?;
                Ok((system, folders, labels))
            },
        )?;

        let system_index = system
            .iter()
            .position(|label| current_label.id == label.id)
            .unwrap_or_default();
        let folder_index = folders
            .iter()
            .position(|label| current_label.id == label.id)
            .unwrap_or_default();
        let labels_index = labels
            .iter()
            .position(|label| current_label.id == label.id)
            .unwrap_or_default();

        Ok(Self {
            system,
            folders,
            labels,
            system_list_state: ScrollableListState::new(Some(system_index)),
            folder_list_state: ScrollableListState::new(Some(folder_index)),
            labels_list_state: ScrollableListState::new(Some(labels_index)),
            active_label: current_label.label_type,
        })
    }

    fn selected_tab_index(&self) -> usize {
        match self.active_label {
            LabelType::Label => 2,
            LabelType::Folder => 1,
            LabelType::System | LabelType::ContactGroup => 0,
        }
    }

    fn selected_label_list(&mut self) -> (&[LocalLabelWithCount], &mut ScrollableListState) {
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

        match key.code {
            KeyCode::Up => {
                let (_, list_state) = self.selected_label_list();
                list_state.prev();
                Command::None
            }
            KeyCode::Down => {
                let (_, list_state) = self.selected_label_list();
                list_state.next();
                Command::None
            }
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

                Command::message(Message::SelectLabel(label.id).into())
            }

            _ => Command::None,
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let [tab_area, list_area] =
            Layout::vertical([Constraint::Length(3), Constraint::Percentage(100)]).areas(area);

        let tabs = Tabs::new(vec!["Default", "Folders", "Labels"])
            .block(Block::new().borders(Borders::ALL))
            .select(self.selected_tab_index())
            .divider(symbols::line::VERTICAL)
            .padding(" ", " ");
        frame.render_widget(tabs, tab_area);

        let (labels, list_state) = self.selected_label_list();

        let items = labels
            .iter()
            .map(|label| {
                let name = label.path.as_deref().unwrap_or(label.name.as_str());
                let text = format!(
                    "[{:04}|{:04}] {name}",
                    label.unread_count, label.total_count
                );
                ListItem::from(text)
            })
            .collect::<Vec<_>>();

        let list = ScrollableList::new(List::new(items));
        frame.render_stateful_widget(list, list_area, list_state);
    }
}
