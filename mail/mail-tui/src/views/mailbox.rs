use crate::events::mailbox::MailboxEvent;
use crate::events::AppEvent;
use crate::state::{LoadingState, MailboxState, MailboxUserContextState, ViewMode};
use crate::view::View;
use crate::views::AppViewContext;
use crate::widgets::{
    ConversationWidget, HelpCategory, HelpItem, LabelWidget, MessageWidget, ScrollableList,
    ScrollableListState, SideBarLabelWidget, WidgetList, WidgetListItem,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use proton_mail_common::db::{LocalConversationId, LocalLabel, LocalLabelId, LocalLabelWithCount};
use proton_mail_common::exports::tracing::warn;
use proton_mail_common::proton_api_mail::domain::LabelType;
use ratatui::layout::{Constraint, Direction, Flex, Layout, Margin, Rect};
use ratatui::prelude::Text;
use ratatui::style::{Color, Style, Stylize};
use ratatui::widgets::{Block, Borders, Clear, HighlightSpacing};
use ratatui::Frame;
use std::ops::Deref;

#[derive(Copy, Clone)]
enum FocusedWidget {
    Labels(LabelType),
    Conversation,
}
enum LabelSelectionMode {
    Label(LocalConversationId, Vec<LocalLabel>),
    Unlabel(LocalConversationId, Vec<LocalLabel>),
    Move(LocalConversationId, Vec<LocalLabel>),
}

pub struct ConversationView {
    item_list_state: ScrollableListState,
    system_labels_list_state: ScrollableListState,
    folder_list_state: ScrollableListState,
    labels_list_state: ScrollableListState,
    focused_widget: FocusedWidget,
    label_selection_mode: Option<LabelSelectionMode>,
    label_selection_list_state: ScrollableListState,
}

impl ConversationView {
    pub fn new() -> Self {
        let mut r = Self {
            item_list_state: ScrollableListState::new(3, Some(0)),
            system_labels_list_state: ScrollableListState::new(1, Some(0)),
            folder_list_state: ScrollableListState::new(1, None),
            labels_list_state: ScrollableListState::new(1, None),
            focused_widget: FocusedWidget::Conversation,
            label_selection_mode: None,
            label_selection_list_state: ScrollableListState::new(1, None),
        };
        r.item_list_state.set_focus_gained();
        r.label_selection_list_state.set_focus_gained();
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

        let [sys_area, _, folder_area, _, label_area] = Layout::vertical([
            Constraint::Min(10),
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
            Constraint::Min(10),
        ])
        .areas(area);

        fn list_from_labels<'a>(
            labels: &'a [LocalLabelWithCount],
            desc: &'static str,
        ) -> ScrollableList<'a, SideBarLabelWidget<'a>> {
            let labels = labels
                .iter()
                .map(|l| WidgetListItem::new(SideBarLabelWidget::new(l)))
                .collect::<Vec<_>>();
            let block = Block::new().borders(Borders::TOP).title(desc);
            ScrollableList::new(WidgetList::new(labels).block(block))
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

        if state.conversation_loading_state() == LoadingState::Loading {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .flex(Flex::Center)
                .constraints([Constraint::Length(1)])
                .split(area);
            frame.render_widget(Text::from("Loading...").centered(), chunks[0]);
            return;
        }

        //TODO: improve
        match &mailbox_context.view_mode {
            ViewMode::Conversations(query) => {
                let conversations = query.value();
                let conversations = conversations.deref();
                self.item_list_state.set_len(conversations.len());
                let list_items = conversations
                    .iter()
                    .enumerate()
                    .map(|(idx, c)| {
                        let item = WidgetListItem::new(ConversationWidget::new(c));
                        let item = if idx % 2 == 0 {
                            item.style(Style::default().bg(Color::LightMagenta))
                        } else {
                            item
                        };
                        if c.num_unread != 0 {
                            item.bold()
                        } else {
                            item
                        }
                    })
                    .collect::<Vec<_>>();
                frame.render_stateful_widget(
                    ScrollableList::new(
                        WidgetList::new(list_items)
                            .highlight_symbol(">> ")
                            .highlight_spacing(HighlightSpacing::Always)
                            .highlight_style(Style {
                                fg: Some(Color::Magenta),
                                bg: Some(Color::White),
                                underline_color: None,
                                add_modifier: Default::default(),
                                sub_modifier: Default::default(),
                            })
                            .block(Block::new().borders(Borders::all())),
                    ),
                    area,
                    &mut self.item_list_state,
                );
            }
            ViewMode::Messages(query) => {
                let messages = query.value();
                let messages = messages.deref();
                self.item_list_state.set_len(messages.len());
                let list_items = messages
                    .iter()
                    .enumerate()
                    .map(|(idx, c)| {
                        let item = WidgetListItem::new(MessageWidget::new(c));
                        let item = if idx % 2 == 0 {
                            item.style(Style::default().bg(Color::LightMagenta))
                        } else {
                            item
                        };
                        if c.unread {
                            item.bold()
                        } else {
                            item
                        }
                    })
                    .collect::<Vec<_>>();
                frame.render_stateful_widget(
                    ScrollableList::new(
                        WidgetList::new(list_items)
                            .highlight_symbol(">> ")
                            .highlight_spacing(HighlightSpacing::Always)
                            .highlight_style(Style {
                                fg: Some(Color::Magenta),
                                bg: Some(Color::White),
                                underline_color: None,
                                add_modifier: Default::default(),
                                sub_modifier: Default::default(),
                            })
                            .block(Block::new().borders(Borders::all())),
                    ),
                    area,
                    &mut self.item_list_state,
                );
            }
        }
    }

    fn draw_label_selection(&mut self, frame: &mut Frame, area: Rect) {
        let Some(selection_mode) = &self.label_selection_mode else {
            return;
        };
        let (title, labels) = match selection_mode {
            LabelSelectionMode::Label(_, l) => ("Select Label to Add", l),
            LabelSelectionMode::Unlabel(_, l) => ("Select Label to Remove", l),
            LabelSelectionMode::Move(_, l) => ("Select Folder to Move into", l),
        };
        let area = area.inner(&Margin {
            horizontal: 10,
            vertical: 5,
        });
        self.label_selection_list_state.set_len(labels.len());
        frame.render_widget(Clear {}, area);
        let labels = labels
            .iter()
            .map(|l| WidgetListItem::new(LabelWidget::new(l)))
            .collect::<Vec<_>>();
        let block = Block::new()
            .borders(Borders::ALL)
            .title(title)
            .bg(Color::Magenta)
            .fg(Color::White);
        let widget = ScrollableList::new(WidgetList::new(labels).block(block));

        frame.render_stateful_widget(widget, area, &mut self.label_selection_list_state);
    }

    fn load_label(&mut self, ctx: &mut AppViewContext, label_id: LocalLabelId) {
        ctx.app_local_dispatcher()
            .queue_event(MailboxEvent::LoadLabelRequest(label_id));
    }

    fn set_focused_widget(&mut self, state: &MailboxState, f: FocusedWidget) {
        let Some(mailbox_context) = state.mailbox_context() else {
            return;
        };
        let current_label_id = if let Some(label) = state.active_label() {
            label
        } else {
            LocalLabelId::new(u64::MAX)
        };

        let (current_label_type, label_index) =
            find_label_index_mailbox(mailbox_context, current_label_id);

        // Correct for focus lost;
        {
            let cur_label_state = self.label_lists_state_mut(current_label_type);
            if cur_label_state.is_focused() {
                cur_label_state.select(Some(label_index));
            }
        }

        self.focused_widget = f;
        self.labels_list_state.set_focus_lost();
        self.system_labels_list_state.set_focus_lost();
        self.folder_list_state.set_focus_lost();
        self.item_list_state.set_focus_lost();

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
            FocusedWidget::Conversation => self.item_list_state.set_focus_gained(),
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

    pub fn with_selected_conversation(
        &mut self,
        ctx: &mut AppViewContext,
        f: impl FnOnce(&mut Self, &mut AppViewContext, LocalConversationId),
    ) {
        if let Some(index) = self.item_list_state.selected() {
            if let Some(id) = if let Some(user_ctx) = ctx.state().mailbox_state.mailbox_context() {
                if let ViewMode::Conversations(q) = &user_ctx.view_mode {
                    q.value().deref().get(index).map(|c| c.id)
                } else {
                    None
                }
            } else {
                None
            } {
                f(self, ctx, id);
            }
        } else {
            warn!("No selected conversation")
        }
    }
}

fn find_label_index_mailbox(
    mbox: &MailboxUserContextState,
    id: LocalLabelId,
) -> (LabelType, usize) {
    let find_fn = |labels: &[LocalLabelWithCount]| -> Option<usize> {
        for (idx, l) in labels.iter().enumerate() {
            if l.id == id {
                return Some(idx);
            }
        }
        None
    };

    if let Some(idx) = find_fn(mbox.system_labels.value().deref()) {
        return (LabelType::System, idx);
    }

    if let Some(idx) = find_fn(mbox.folders.value().deref()) {
        return (LabelType::Folder, idx);
    }

    if let Some(idx) = find_fn(mbox.labels.value().deref()) {
        return (LabelType::Label, idx);
    }

    (LabelType::System, 0)
}

fn find_label_index(labels: &[LocalLabelWithCount], id: LocalLabelId) -> usize {
    for (idx, l) in labels.iter().enumerate() {
        if l.id == id {
            return idx;
        }
    }
    0
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

        self.draw_label_selection(frame, area);
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
                    HelpItem {
                        key: "P",
                        description: "Poll Events",
                    },
                    HelpItem {
                        key: "Ctrl+P",
                        description: "Flush Queue",
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
                    HelpItem {
                        key: "d",
                        description: "Delete selected label",
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
                    HelpItem {
                        key: "d",
                        description: "Delete selected conversation",
                    },
                    HelpItem {
                        key: "u",
                        description: "Mark selected conversation unread",
                    },
                    HelpItem {
                        key: "r",
                        description: "Mark selected conversation read",
                    },
                    HelpItem {
                        key: "l",
                        description: "Label selected conversation",
                    },
                    HelpItem {
                        key: "k",
                        description: "UnLabel selected conversation",
                    },
                    HelpItem {
                        key: "m",
                        description: "Move selected conversation",
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

            if k.code == KeyCode::Char('p') && k.kind == KeyEventKind::Press {
                if k.modifiers == KeyModifiers::CONTROL {
                    ctx.app_local_dispatcher()
                        .queue_event(MailboxEvent::ExecQueue);
                } else {
                    ctx.app_local_dispatcher()
                        .queue_event(MailboxEvent::PollEventLoop);
                }
                return;
            }

            if let Some(selection) = &mut self.label_selection_mode {
                match k.code {
                    KeyCode::Esc => {
                        self.label_selection_mode = None;
                    }
                    KeyCode::Enter => {
                        let Some(index) = self.label_selection_list_state.selected() else {
                            return;
                        };
                        match selection {
                            LabelSelectionMode::Label(id, labels) => {
                                let label_id = labels[index].id;
                                ctx.app_local_dispatcher()
                                    .queue_event(MailboxEvent::LabelConversation(*id, label_id));
                            }
                            LabelSelectionMode::Unlabel(id, labels) => {
                                let label_id = labels[index].id;
                                ctx.app_local_dispatcher()
                                    .queue_event(MailboxEvent::UnlabelConversation(*id, label_id));
                            }
                            LabelSelectionMode::Move(id, labels) => {
                                let label_id = labels[index].id;
                                ctx.app_local_dispatcher()
                                    .queue_event(MailboxEvent::MoveConversation(*id, label_id));
                            }
                        };
                        self.label_selection_mode = None;
                    }
                    KeyCode::Up => {
                        self.label_selection_list_state.prev();
                    }
                    KeyCode::Down => {
                        self.label_selection_list_state.next();
                    }
                    _ => {}
                }
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
                                        self.load_label(ctx, label.id);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                FocusedWidget::Conversation => match k.code {
                    KeyCode::Up => {
                        self.item_list_state.prev();
                    }
                    KeyCode::Down => {
                        self.item_list_state.next();
                    }
                    KeyCode::Left => {
                        let mailbox_state = &ctx.state().mailbox_state;
                        self.set_focused_widget(
                            mailbox_state,
                            FocusedWidget::Labels(LabelType::System),
                        );
                    }
                    KeyCode::Char('d') => {
                        self.with_selected_conversation(ctx, |_, ctx, id| {
                            ctx.app_local_dispatcher()
                                .queue_event(MailboxEvent::DeleteConversation(id))
                        });
                    }
                    KeyCode::Char('u') => {
                        self.with_selected_conversation(ctx, |_, ctx, id| {
                            ctx.app_local_dispatcher()
                                .queue_event(MailboxEvent::MarkConversationUnread(id))
                        });
                    }
                    KeyCode::Char('r') => {
                        self.with_selected_conversation(ctx, |_, ctx, id| {
                            ctx.app_local_dispatcher()
                                .queue_event(MailboxEvent::MarkConversationRead(id))
                        });
                    }
                    KeyCode::Char('l') => {
                        self.with_selected_conversation(ctx, |this, ctx, id| {
                            if let Some(state) = ctx.state().mailbox_state.mailbox_context() {
                                let labels = match state
                                    .mailbox
                                    .user_context()
                                    .get_labels_by_type(LabelType::Label)
                                {
                                    Ok(l) => l,
                                    Err(e) => {
                                        ctx.app_local_dispatcher()
                                            .set_error("Failed to get movable folder list", e);
                                        return;
                                    }
                                };
                                this.label_selection_mode =
                                    Some(LabelSelectionMode::Label(id, labels));
                                this.label_selection_list_state.select(Some(0));
                                this.label_selection_list_state.set_offset(0);
                            }
                        });
                    }
                    KeyCode::Char('k') => {
                        self.with_selected_conversation(ctx, |this, ctx, id| {
                            if let Some(state) = ctx.state().mailbox_state.mailbox_context() {
                                let labels = match state
                                    .mailbox
                                    .user_context()
                                    .get_labels_by_type(LabelType::Label)
                                {
                                    Ok(l) => l,
                                    Err(e) => {
                                        ctx.app_local_dispatcher()
                                            .set_error("Failed to get movable folder list", e);
                                        return;
                                    }
                                };
                                this.label_selection_mode =
                                    Some(LabelSelectionMode::Unlabel(id, labels));
                                this.label_selection_list_state.select(Some(0));
                                this.label_selection_list_state.set_offset(0);
                            }
                        });
                    }
                    KeyCode::Char('m') => {
                        self.with_selected_conversation(ctx, |this, ctx, id| {
                            if let Some(state) = ctx.state().mailbox_state.mailbox_context() {
                                let labels = match state.mailbox.user_context().movable_folders() {
                                    Ok(l) => l,
                                    Err(e) => {
                                        ctx.app_local_dispatcher()
                                            .set_error("Failed to get movable folder list", e);
                                        return;
                                    }
                                };
                                this.label_selection_mode =
                                    Some(LabelSelectionMode::Move(id, labels));
                                this.label_selection_list_state.select(Some(0));
                                this.label_selection_list_state.set_offset(0);
                            }
                        });
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
