use crate::events::mailbox::MailboxEvents;
use crate::events::AppEvents;
use crate::state::{DataLoadError, LoginState, MailboxState};
use crate::tui_utils::inset_rect;
use crate::view::View;
use crate::views::AppViewContext;
use crate::widgets::{ScrollableList, ScrollableListState};
use anyhow::anyhow;
use crossterm::event::{Event, KeyCode, KeyModifiers};
use proton_api_mail::domain::LabelType;
use proton_mail_db::{LocalLabel, LocalLabelId};
use ratatui::layout::{Constraint, Direction, Flex, Layout, Rect};
use ratatui::prelude::Text;
use ratatui::style::{Color, Style, Stylize};
use ratatui::widgets::{Block, Borders, HighlightSpacing, List, ListItem};
use ratatui::Frame;

#[derive(Copy, Clone, Eq, PartialEq)]
enum LoadingState {
    Unloaded,
    LoadingLabels,
    LoadingConversations,
    Done,
}

#[derive(Copy, Clone)]
enum FocusedWidget {
    Labels(LabelType),
    Conversation,
}

pub struct ConversationView {
    state: LoadingState,
    conversation_list_state: ScrollableListState,
    system_labels_list_state: ScrollableListState,
    folder_list_state: ScrollableListState,
    labels_list_state: ScrollableListState,
    focused_widget: FocusedWidget,
}

impl ConversationView {
    pub fn new() -> Self {
        let mut r = Self {
            state: LoadingState::Unloaded,
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
            labels: &'a [LocalLabel],
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
                    ListItem::new(Text::from(name).not_bold())
                })
                .collect::<Vec<_>>();
            let block = Block::new().borders(Borders::TOP).title(desc).bold();
            ScrollableList::new(List::new(labels).block(block))
        }

        // system labels
        {
            frame.render_stateful_widget(
                list_from_labels(state.label_list(LabelType::System), "System"),
                sys_area,
                &mut self.system_labels_list_state,
            );
        }

        // Folders
        {
            frame.render_stateful_widget(
                list_from_labels(state.label_list(LabelType::Folder), "Folders"),
                folder_area,
                &mut self.folder_list_state,
            );
        }
        // Labels
        {
            frame.render_stateful_widget(
                list_from_labels(state.label_list(LabelType::Label), "Labels"),
                label_area,
                &mut self.labels_list_state,
            );
        }
    }

    fn draw_conversation_area(&mut self, state: &MailboxState, frame: &mut Frame, area: Rect) {
        let list_items = state
            .conversation_list
            .iter()
            .enumerate()
            .map(|(idx, conv)| {
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
                            .title(format!(" {} ", state.active_label_name()))
                            .borders(Borders::all()),
                    ),
            ),
            area,
            &mut self.conversation_list_state,
        );
    }

    fn load_data(&mut self, ctx: &mut AppViewContext) {
        let state = ctx.state();
        let LoginState::LoggedIn(user_state) = &state.login_state else {
            ctx.set_error(
                "Session Error",
                DataLoadError::Other(anyhow!("No active session")),
            );
            return;
        };
        self.state = LoadingState::LoadingLabels;
        state
            .mailbox_state
            .first_load(user_state, ctx.dispatcher(), &state.runtime);
    }

    fn load_label(&mut self, ctx: &mut AppViewContext, label: LocalLabel) {
        let dispatcher = ctx.dispatcher();
        let state = ctx.state_mut();
        let LoginState::LoggedIn(user_state) = &state.login_state else {
            ctx.set_error(
                "Session Error",
                DataLoadError::Other(anyhow!("No active session")),
            );
            return;
        };
        self.state = LoadingState::LoadingConversations;
        state
            .mailbox_state
            .load_label(label, user_state, dispatcher, &state.runtime);
    }

    fn update_label_lists_size(&mut self, state: &MailboxState) {
        self.system_labels_list_state
            .set_len(state.label_list(LabelType::System).len());
        self.folder_list_state
            .set_len(state.label_list(LabelType::Folder).len());
        self.labels_list_state
            .set_len(state.label_list(LabelType::Label).len());
    }

    fn update_conversation_list(&mut self, state: &MailboxState) {
        if state.conversation_list.is_empty() {
            self.conversation_list_state.set_len(0);
            return;
        }

        self.conversation_list_state
            .set_len(state.conversation_list.len());
    }

    fn set_focused_widget(&mut self, state: &MailboxState, f: FocusedWidget) {
        let current_label_type = state.active_label().label_type;

        // Correct for focus lost;
        {
            let cur_label_state = self.label_lists_state_mut(current_label_type);
            if cur_label_state.is_focused() {
                cur_label_state.select(Some(find_label_index(
                    state.label_list(current_label_type),
                    state.active_label().id,
                )));
            }
        }

        self.focused_widget = f;
        self.labels_list_state.set_focus_lost();
        self.system_labels_list_state.set_focus_lost();
        self.folder_list_state.set_focus_lost();
        self.conversation_list_state.set_focus_lost();

        fn find_label_index(labels: &[LocalLabel], id: LocalLabelId) -> usize {
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
                let list_state = self.label_lists_state_mut(label_type);
                list_state.set_focus_gained();
                list_state.select(Some(find_label_index(
                    state.label_list(label_type),
                    state.active_label().id,
                )));
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

impl View<AppViewContext, AppEvents> for ConversationView {
    fn on_enter(&mut self, ctx: &mut AppViewContext) {
        let is_unloaded = matches!(self.state, LoadingState::Unloaded);
        if is_unloaded {
            self.load_data(ctx);
        }
    }

    fn on_exit(&mut self, _: &mut AppViewContext) {}

    fn draw(&mut self, state: &AppViewContext, frame: &mut Frame, area: Rect) {
        let [label_area, conversation_area] =
            Layout::horizontal([Constraint::Max(30), Constraint::Min(50)]).areas(area);

        let state = state.state();
        self.draw_label_area(&state.mailbox_state, frame, label_area);

        match &self.state {
            LoadingState::Unloaded => {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .flex(Flex::Center)
                    .constraints([Constraint::Length(1)])
                    .split(conversation_area);
                frame.render_widget(Text::from("Unloaded").centered(), chunks[0]);
            }
            LoadingState::LoadingLabels => {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .flex(Flex::Center)
                    .constraints([Constraint::Length(1)])
                    .split(conversation_area);
                frame.render_widget(Text::from("Loading Labels...").centered(), chunks[0]);
            }
            LoadingState::LoadingConversations => {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .flex(Flex::Center)
                    .constraints([Constraint::Length(1)])
                    .split(conversation_area);
                frame.render_widget(Text::from("Loading Conversations...").centered(), chunks[0]);
            }
            LoadingState::Done => {
                self.draw_conversation_area(&state.mailbox_state, frame, conversation_area);
            }
        }
    }

    fn draw_help(&self, _: &AppViewContext, frame: &mut Frame, area: Rect) {
        let shared_text = "(Ctrl+r) Reload|(Ctrl+l) Logout|(▲) Up|(▼) Down|";
        let [shared, context_area, _] = Layout::horizontal([
            Constraint::Length(u16::try_from(shared_text.len()).expect("exceeds capacity")),
            Constraint::Min(10),
            Constraint::Length(1),
        ])
        .areas(area);

        frame.render_widget(Text::from(shared_text), shared);

        match self.focused_widget {
            FocusedWidget::Labels(_) => {
                frame.render_widget(
                    Text::from("(►) Conversations|(Tab) Switch Label Type|(Enter) Select Label"),
                    context_area,
                );
            }
            FocusedWidget::Conversation => {
                frame.render_widget(Text::from("(◄) Labels|(Enter) Open"), context_area);
            }
        }
    }

    fn on_event(&mut self, ctx: &mut AppViewContext, event: AppEvents) -> Option<AppEvents> {
        match event {
            AppEvents::Mailbox(e) => {
                match e {
                    MailboxEvents::LoadLabels(r) => match r {
                        Ok(labels) => {
                            ctx.state_mut().mailbox_state.assign_labels(labels);
                            self.update_label_lists_size(&ctx.state().mailbox_state);
                            self.state = LoadingState::LoadingConversations;
                        }
                        Err(e) => {
                            ctx.set_error("Failed to load labels", e);
                        }
                    },
                    MailboxEvents::LoadConversations(r) => match r {
                        Ok(conv) => {
                            ctx.state_mut().mailbox_state.conversation_list = conv;
                            self.conversation_list_state.select(Some(0));
                            self.update_conversation_list(&ctx.state().mailbox_state);
                            self.state = LoadingState::Done;
                        }
                        Err(e) => {
                            ctx.set_error("Failed to load conversations", e);
                        }
                    },
                }
                None
            }

            _ => Some(event),
        }
    }

    fn on_input(&mut self, ctx: &mut AppViewContext, event: &Event) {
        if let Event::Key(k) = event {
            if k.code == KeyCode::Char('r') && k.modifiers == KeyModifiers::CONTROL {
                ctx.state_mut().mailbox_state.reset();
                self.conversation_list_state.select(Some(0));
                self.load_data(ctx);
                return;
            } else if k.code == KeyCode::Char('l') && k.modifiers == KeyModifiers::CONTROL {
                ctx.pop_view();
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
                            let mailbox_state = &mut ctx.state_mut().mailbox_state;
                            self.set_focused_widget(mailbox_state, FocusedWidget::Conversation);
                        }
                        KeyCode::Tab => {
                            let mailbox_state = &mut ctx.state_mut().mailbox_state;
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
                                let mailbox_state = &mut ctx.state_mut().mailbox_state;
                                let label =
                                    mailbox_state.label_list(label_type).get(index).cloned();
                                if let Some(label) = label {
                                    self.load_label(ctx, label);
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
                        let mailbox_state = &mut ctx.state_mut().mailbox_state;
                        self.set_focused_widget(
                            mailbox_state,
                            FocusedWidget::Labels(mailbox_state.active_label().label_type),
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
