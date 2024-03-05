use crate::events::mailbox::MailboxEvent;
use crate::events::AppEvent;
use crate::state::{LoadingState, MailboxState, MailboxUserContextState};
use crate::tui_utils::inset_rect;
use crate::view::View;
use crate::views::AppViewContext;
use crate::widgets::{HelpCategory, HelpItem, ScrollableList, ScrollableListState};
use crossterm::event::{Event, KeyCode, KeyModifiers};
use proton_mail_common::proton_api_mail::domain::LabelType;
use proton_mail_common::proton_mail_db::{LocalLabel, LocalLabelId, LocalLabelWithCount};
use ratatui::layout::{Constraint, Direction, Flex, Layout, Rect};
use ratatui::prelude::Text;
use ratatui::style::{Color, Style, Stylize};
use ratatui::widgets::{Block, Borders, HighlightSpacing, List, ListItem};
use ratatui::Frame;
use std::ops::Deref;

#[derive(Copy, Clone)]
enum FocusedWidget {
    Labels(LabelType),
    Conversation,
}

pub struct ConversationView {
    conversation_list_state: ScrollableListState,
    system_labels_list_state: ScrollableListState,
    folder_list_state: ScrollableListState,
    labels_list_state: ScrollableListState,
    focused_widget: FocusedWidget,
}

impl ConversationView {
    pub fn new() -> Self {
        let mut r = Self {
            conversation_list_state: ScrollableListState::new(2, Some(0)),
            system_labels_list_state: ScrollableListState::new(1, Some(0)),
            folder_list_state: ScrollableListState::new(1, None),
            labels_list_state: ScrollableListState::new(1, None),
            focused_widget: FocusedWidget::Conversation,
        };
        r.conversation_list_state.set_focus_gained();
        r
    }

    fn draw_label_area(&mut self, state: &MailboxState, frame: &mut Frame, area: Rect) {
        if state.labels_loading_state() == LoadingState::Loading {
            let [chunk] = Layout::default()
                .direction(Direction::Vertical)
                .flex(Flex::Center)
                .constraints([Constraint::Length(1)])
                .areas(area);
            frame.render_widget(Text::from("Loading...").centered(), chunk);
            return;
        }

        let Some(mailbox_context) = state.mailbox_context() else {
            return;
        };

        let labels_block = Block::new().borders(Borders::RIGHT);
        frame.render_widget(labels_block, area);
        let internal_area = inset_rect(area, 1);
        let [sys_area, _, folder_area, _, label_area] = Layout::vertical([
            Constraint::Percentage(20),
            Constraint::Length(1),
            Constraint::Percentage(40),
            Constraint::Length(1),
            Constraint::Min(10),
        ])
        .areas(internal_area);

        fn list_from_labels<'a>(
            labels: &'a [LocalLabelWithCount],
            desc: &'static str,
        ) -> ScrollableList<'a> {
            let labels = labels
                .iter()
                .map(|l| {
                    let name = if let Some(path) = &l.path {
                        path.as_str()
                    } else {
                        l.name.as_str()
                    };
                    ListItem::new(Text::from(format!("[{}] {name}", l.unread_count)).not_bold())
                })
                .collect::<Vec<_>>();
            let block = Block::new().borders(Borders::TOP).title(desc).bold();
            ScrollableList::new(List::new(labels).block(block))
        }

        // system labels
        {
            let labels = mailbox_context.system_labels.value();
            let labels = labels.deref();
            self.system_labels_list_state.set_len(labels.len());
            frame.render_stateful_widget(
                list_from_labels(labels, "System"),
                sys_area,
                &mut self.system_labels_list_state,
            );
        }

        // Folders
        {
            let labels = mailbox_context.folders.value();
            let labels = labels.deref();
            self.folder_list_state.set_len(labels.len());
            frame.render_stateful_widget(
                list_from_labels(labels, "Folders"),
                folder_area,
                &mut self.folder_list_state,
            );
        }
        // Labels
        {
            let labels = mailbox_context.labels.value();
            let labels = labels.deref();
            self.labels_list_state.set_len(labels.len());
            frame.render_stateful_widget(
                list_from_labels(labels, "Labels"),
                label_area,
                &mut self.labels_list_state,
            );
        }
    }

    fn draw_conversation_area(&mut self, state: &MailboxState, frame: &mut Frame, area: Rect) {
        let Some(mailbox_context) = state.mailbox_context() else {
            return;
        };
        let active_label_name = state.active_label_name();

        if state.conversation_loading_state() == LoadingState::Loading {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .flex(Flex::Center)
                .constraints([Constraint::Length(1)])
                .split(area);
            frame.render_widget(Text::from("Loading...").centered(), chunks[0]);
            return;
        }

        let conversations = mailbox_context.conversations.value();
        let conversations = conversations.deref();
        self.conversation_list_state.set_len(conversations.len());
        let list_items = conversations.iter().enumerate().map(|(idx, conv)| {
            let senders = {
                if conv.senders.len() == 1 {
                    conv.senders[0].name.clone()
                } else {
                    conv.senders
                        .iter()
                        .map(|s| s.name.clone())
                        .collect::<Vec<_>>()
                        .join(",")
                }
            };
            let line = Text::from(vec![
                if conv.num_messages > 1 {
                    format!("[{}] {}", conv.num_messages, senders).into()
                } else {
                    senders.into()
                },
                conv.subject.clone().into(),
            ]);
            let item = ListItem::new(line);
            let item = if idx % 2 == 0 {
                item.on_light_magenta()
            } else {
                item
            };
            if conv.num_unread != 0 {
                item.bold()
            } else {
                item
            }
        });

        frame.render_stateful_widget(
            ScrollableList::new(
                List::new(list_items)
                    .highlight_symbol(">> ")
                    .highlight_spacing(HighlightSpacing::Always)
                    .highlight_style(Style {
                        fg: Some(Color::Magenta),
                        bg: Some(Color::White),
                        underline_color: None,
                        add_modifier: Default::default(),
                        sub_modifier: Default::default(),
                    })
                    .block(
                        Block::new()
                            .title(format!(" {} ", active_label_name))
                            .borders(Borders::all()),
                    ),
            ),
            area,
            &mut self.conversation_list_state,
        );
    }

    fn load_label(&mut self, ctx: &mut AppViewContext, label: LocalLabel) {
        ctx.app_local_dispatcher()
            .queue_event(MailboxEvent::LoadLabelRequest(label));
    }

    fn set_focused_widget(&mut self, state: &MailboxState, f: FocusedWidget) {
        let Some(mailbox_context) = state.mailbox_context() else {
            return;
        };
        let (current_label_type, current_label_id) = if let Some(label) = state.active_label() {
            (label.label_type, label.id)
        } else {
            (LabelType::System, LocalLabelId::new(u64::MAX))
        };

        // Correct for focus lost;
        {
            let selection_index = Some(find_label_index(
                labels_for_type(mailbox_context, current_label_type).deref(),
                current_label_id,
            ));
            let cur_label_state = self.label_lists_state_mut(current_label_type);
            if cur_label_state.is_focused() {
                cur_label_state.select(selection_index);
            }
        }

        self.focused_widget = f;
        self.labels_list_state.set_focus_lost();
        self.system_labels_list_state.set_focus_lost();
        self.folder_list_state.set_focus_lost();
        self.conversation_list_state.set_focus_lost();

        fn find_label_index(labels: &[LocalLabelWithCount], id: LocalLabelId) -> usize {
            for (idx, l) in labels.iter().enumerate() {
                if l.id == id {
                    return idx;
                }
            }
            0
        }

        match current_label_type {
            LabelType::Label => {
                self.system_labels_list_state.select(None);
                self.system_labels_list_state.set_offset(0);
                self.folder_list_state.select(None);
                self.folder_list_state.set_offset(0);
            }
            LabelType::Folder => {
                self.system_labels_list_state.select(None);
                self.system_labels_list_state.set_offset(0);
                self.labels_list_state.select(None);
                self.labels_list_state.set_offset(0);
            }
            LabelType::System => {
                self.folder_list_state.select(None);
                self.folder_list_state.set_offset(0);
                self.labels_list_state.select(None);
                self.labels_list_state.set_offset(0);
            }
            _ => {
                unreachable!()
            }
        }

        match self.focused_widget {
            FocusedWidget::Labels(label_type) => {
                let selection = Some(find_label_index(
                    labels_for_type(mailbox_context, current_label_type).deref(),
                    current_label_id,
                ));
                let list_state = self.label_lists_state_mut(label_type);
                list_state.set_focus_gained();
                list_state.select(selection);
            }
            FocusedWidget::Conversation => self.conversation_list_state.set_focus_gained(),
        }
    }

    fn label_lists_state_mut(&mut self, label_type: LabelType) -> &mut ScrollableListState {
        match label_type {
            LabelType::Label => &mut self.labels_list_state,
            LabelType::Folder => &mut self.folder_list_state,
            LabelType::System => &mut self.system_labels_list_state,
            _ => {
                unreachable!()
            }
        }
    }
}

fn labels_for_type(
    context: &MailboxUserContextState,
    label_type: LabelType,
) -> impl Deref<Target = Vec<LocalLabelWithCount>> + '_ {
    match label_type {
        LabelType::Label => context.labels.value(),
        LabelType::ContactGroup => {
            unreachable!()
        }
        LabelType::Folder => context.folders.value(),
        LabelType::System => context.system_labels.value(),
    }
}

impl View<AppViewContext, AppEvent> for ConversationView {
    fn draw(&mut self, state: &AppViewContext, frame: &mut Frame, area: Rect) {
        let [label_area, conversation_area] =
            Layout::horizontal([Constraint::Max(30), Constraint::Min(50)]).areas(area);

        let state = state.state();
        self.draw_label_area(&state.mailbox_state, frame, label_area);

        self.draw_conversation_area(&state.mailbox_state, frame, conversation_area);
    }

    fn help_items(&self) -> &[HelpCategory] {
        static ITEMS: [HelpCategory; 3] = [
            HelpCategory {
                name: "Mailbox",
                items: &[
                    HelpItem {
                        key: "Ctrl+R",
                        description: "Reload",
                    },
                    HelpItem {
                        key: "Ctrl+L",
                        description: "Logout",
                    },
                    HelpItem {
                        key: "▲",
                        description: "Previous Item",
                    },
                    HelpItem {
                        key: "▼",
                        description: "Next Item",
                    },
                ],
            },
            HelpCategory {
                name: "Labels",
                items: &[
                    HelpItem {
                        key: "Enter",
                        description: "Select Label",
                    },
                    HelpItem {
                        key: "Tab",
                        description: "Next Category",
                    },
                    HelpItem {
                        key: "►",
                        description: "Go To Conversations",
                    },
                ],
            },
            HelpCategory {
                name: "Conversation",
                items: &[
                    HelpItem {
                        key: "Ctrl+R",
                        description: "Reload",
                    },
                    HelpItem {
                        key: "Ctrl+L",
                        description: "Logout",
                    },
                    HelpItem {
                        key: "◄",
                        description: "Go to Labels",
                    },
                ],
            },
        ];

        &ITEMS
    }

    fn on_input(&mut self, ctx: &mut AppViewContext, event: &Event) {
        if let Event::Key(k) = event {
            if k.code == KeyCode::Char('r') && k.modifiers == KeyModifiers::CONTROL {
                ctx.app_local_dispatcher()
                    .queue_event(MailboxEvent::MailboxRefresh);
                return;
            } else if k.code == KeyCode::Char('l') && k.modifiers == KeyModifiers::CONTROL {
                ctx.app_local_dispatcher().queue_event(MailboxEvent::Logout);
                return;
            }

            match self.focused_widget {
                FocusedWidget::Labels(label_type) => {
                    let state = self.label_lists_state_mut(label_type);
                    match k.code {
                        KeyCode::Up => {
                            state.prev();
                        }
                        KeyCode::Down => {
                            state.next();
                        }
                        KeyCode::Right => {
                            let mailbox_state = &ctx.state().mailbox_state;
                            self.set_focused_widget(mailbox_state, FocusedWidget::Conversation);
                        }
                        KeyCode::Tab => {
                            let mailbox_state = &ctx.state().mailbox_state;
                            match label_type {
                                LabelType::Label => {
                                    self.set_focused_widget(
                                        mailbox_state,
                                        FocusedWidget::Labels(LabelType::System),
                                    );
                                }
                                LabelType::Folder => {
                                    self.set_focused_widget(
                                        mailbox_state,
                                        FocusedWidget::Labels(LabelType::Label),
                                    );
                                }
                                LabelType::System => {
                                    self.set_focused_widget(
                                        mailbox_state,
                                        FocusedWidget::Labels(LabelType::Folder),
                                    );
                                }
                                _ => {}
                            };
                        }
                        KeyCode::Enter => {
                            if let Some(index) = state.selected() {
                                if let Some(mailbox_context) =
                                    ctx.state().mailbox_state.mailbox_context()
                                {
                                    let label = labels_for_type(mailbox_context, label_type)
                                        .deref()
                                        .get(index)
                                        .cloned();
                                    if let Some(label) = label {
                                        self.load_label(ctx, label.into());
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                FocusedWidget::Conversation => match k.code {
                    KeyCode::Up => {
                        self.conversation_list_state.prev();
                    }
                    KeyCode::Down => {
                        self.conversation_list_state.next();
                    }
                    KeyCode::Left => {
                        let mailbox_state = &ctx.state().mailbox_state;
                        self.set_focused_widget(
                            mailbox_state,
                            FocusedWidget::Labels(mailbox_state.active_label_type()),
                        );
                    }
                    _ => {}
                },
            }
        }
    }

    fn name(&self) -> &'static str {
        "Mailbox"
    }
}
