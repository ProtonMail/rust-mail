use super::search::{Search, SearchStatusBar};
use crate::app::Command;
use crate::app_model::mailbox::composer::Composer;
use crate::app_model::mailbox::conversations::ConversationsState;
use crate::app_model::mailbox::messages::MessagesState;
use crate::app_model::mailbox::popups::{
    CustomSnoozeOption, LabelItemPopup, LabelSelectPopup, MoveItemPopup, SnoozeItemPopup,
};
use crate::app_model::mailbox::{Items, Message, poll_event_loop, refresh};
use crate::app_model::watcher::TuiWatchHandle;
use crate::app_model::{AppState, AppStateHandler, HelpPopup, YesNoPopup};
use crate::messages::Messages;
use crate::widgets::{CenteredThrobber, ScrollableListState};
use anyhow::anyhow;
use crossterm::event::{KeyCode, KeyModifiers};
use flume::Sender;
use futures::FutureExt;

use crate::widgets::utils::date_from_timestamp;
use proton_action_queue::observers::{ActionFailureObserver, ActionFailureReason};
use proton_action_queue::queue::{ActionError, AsActionError};
use proton_core_common::actions::event_poll::EventPoll;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{Label, ModelExtension};
use proton_mail_common::datatypes::{ReadFilter, SystemLabelId, ViewMode};
use proton_mail_common::draft::Draft;
use proton_mail_common::draft::observers::{DraftSendResultWatcher, DraftSendResultWatcherMode};
use proton_mail_common::models::{
    DraftSendFailure, DraftSendResult, DraftSendResultOrigin, LabelWithCounters,
};
use proton_mail_common::proton_mail_api::proton_core_api::services::proton::LabelId;
use proton_mail_common::{
    AppError, MailContext, MailContextError, MailContextResult, MailUserContext, Mailbox,
};
use ratatui::crossterm::event::Event;
use ratatui::layout::{Flex, Rect};
use ratatui::prelude::*;
use std::sync::Arc;
use throbber_widgets_tui::ThrobberState;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

enum State {
    Syncing(ThrobberState),
    Conversations(ConversationsState),
    Messages(MessagesState),
}

impl State {
    fn new_syncing() -> Self {
        Self::Syncing(ThrobberState::default())
    }
}

pub struct MailboxModel {
    ctx: Arc<MailUserContext>,
    mailbox: Mailbox,
    label: LabelWithCounters,
    label_watcher: Option<TuiWatchHandle>,
    state: State,
    cancel_token: CancellationToken,
    composer: Option<Composer>,
    search: Option<Search>,
    search_status: Option<SearchStatusBar>,
    filter: ReadFilter,
    background_worker_initialized: bool,
}

impl MailboxModel {
    pub async fn new(ctx: Arc<MailUserContext>) -> MailContextResult<Self> {
        let stash = ctx.user_stash();
        let tether = stash.connection().await?;
        let mailbox = Mailbox::with_remote_id(&tether, LabelId::inbox()).await?;

        let label = LabelWithCounters::load(mailbox.label_id(), &tether)
            .await?
            .ok_or(AppError::LabelNotFound(mailbox.label_id()))?;

        Ok(Self {
            ctx,
            mailbox,
            state: State::new_syncing(),
            label,
            cancel_token: CancellationToken::new(),
            label_watcher: None,
            composer: None,
            search: None,
            search_status: None,
            filter: ReadFilter::All,
            background_worker_initialized: false,
        })
    }

    pub fn ctx(&self) -> Arc<MailUserContext> {
        Arc::clone(&self.ctx)
    }

    fn create_background_worker(&mut self) -> Command<Messages> {
        let ctx = self.ctx.clone();
        if self.background_worker_initialized {
            Command::none()
        } else {
            self.background_worker_initialized = true;
            background_worker(ctx, self.cancel_token.clone())
        }
    }

    #[must_use]
    fn sync_mailbox(&mut self, mbox: Mailbox) -> Command<Messages> {
        self.state = State::new_syncing();
        let filter = self.filter;
        let ctx = Arc::clone(&self.ctx);

        // Create the background worker.
        Command::batch([
            self.create_background_worker(),
            Command::task(async move {
                let stash = ctx.user_stash();
                let Ok(tether) = stash.connection().await else {
                    return Command::message(Messages::DisplayError(
                        None,
                        anyhow!("Failed to acquire db connection"),
                    ));
                };
                let label = match LabelWithCounters::load(mbox.label_id(), &tether).await {
                    Ok(Some(label)) => label,
                    Ok(None) => {
                        let e = anyhow!(
                            "Failed to get label: {}",
                            AppError::LabelNotFound(mbox.label_id())
                        );
                        error!("{e:?}");
                        return Command::message(Messages::DisplayError(None, e));
                    }
                    Err(e) => {
                        let e = anyhow!("Failed to get label: {e}");
                        error!("{e:?}");
                        return Command::message(Messages::DisplayError(None, e));
                    }
                };

                if mbox.view_mode() == ViewMode::Conversations {
                    ConversationsState::build(Arc::clone(&ctx), mbox, label, filter)
                } else {
                    MessagesState::build(Arc::clone(&ctx), mbox, label, filter)
                }
            }),
        ])
    }

    fn build_item_count_query(&mut self) -> Command<Messages> {
        let label_id = self.label.local_id.unwrap();
        let stash = self.ctx.user_stash().to_owned();
        Command::task(async move {
            let Ok(tether) = stash.connection().await else {
                return Command::message(Messages::DisplayError(
                    None,
                    anyhow!("Failed to acquire db connection"),
                ));
            };
            let label = Label::find_by_id(label_id, &tether).await;
            let handle = LabelWithCounters::watch(&stash);
            let label_and_recevier = label.and_then(|l| handle.map(|h| (l, h)));
            match label_and_recevier {
                Ok((label, handle)) => {
                    if let Some(label) = label {
                        let (watcher, background_command) =
                            TuiWatchHandle::from_watcher_handle(handle, move || {
                                let stash = stash.clone();
                                async move {
                                    let Ok(tether) = stash.connection().await else {
                                        return Some(Messages::DisplayError(
                                            None,
                                            anyhow!("Failed to acquire db connection"),
                                        ));
                                    };
                                    let label_id = label.local_id.unwrap();
                                    LabelWithCounters::load(label_id, &tether)
                                        .await
                                        .inspect_err(|e| {
                                            tracing::error!("Failed to get label: `{e:?}`");
                                        })
                                        .ok()
                                        .flatten()
                                        .map(|label| Message::LabelRefreshed(label).into())
                                        .or_else(|| {
                                            tracing::warn!(
                                                "Received change which deleted current label"
                                            );
                                            None
                                        })
                                }
                                .boxed()
                            });
                        Command::batch([
                            Command::message(Message::NewLabelWatcher(watcher)),
                            background_command,
                        ])
                    } else {
                        Command::message(MailContextError::from(AppError::LabelNotFound(label_id)))
                    }
                }
                Err(e) => Command::message(MailContextError::from(e)),
            }
        })
    }

    fn open_conversation_view(
        &mut self,
        mbox: Mailbox,
        label: LabelWithCounters,
        state: ConversationsState,
    ) -> Command<Messages> {
        self.mailbox = mbox;
        self.label = label;
        self.state = State::Conversations(state);
        self.build_item_count_query()
    }

    fn open_message_view(
        &mut self,
        mbox: Mailbox,
        label: LabelWithCounters,
        state: MessagesState,
    ) -> Command<Messages> {
        self.mailbox = mbox;
        self.label = label;
        self.state = State::Messages(state);
        tracing::info!("message viewopen");
        self.build_item_count_query()
    }

    fn open_search_view(&mut self, mbox: Mailbox, state: MessagesState) -> Command<Messages> {
        self.mailbox = mbox;
        self.state = State::Messages(state);
        self.build_item_count_query()
    }

    fn open_label_select_popup(&mut self) -> Command<Messages> {
        let ctx = Arc::clone(&self.ctx);
        let label = self.label.clone();
        let view_mode = self.mailbox.view_mode();
        Command::task(async move {
            match LabelSelectPopup::new(ctx, &label, view_mode).await {
                Ok(state) => Command::message(Messages::RaisePopup(Box::new(state))),
                Err(e) => {
                    let e = anyhow!("Failed to load labels: {e}");
                    tracing::error!("{e:?}");
                    Command::message(Messages::DisplayError(None, e))
                }
            }
        })
    }

    fn open_move_item_popup(&mut self, item: Items) -> Command<Messages> {
        if matches!(&self.state, State::Syncing(_)) {
            return Command::None;
        }

        let ctx = Arc::clone(&self.ctx);
        Command::task(async move {
            match MoveItemPopup::new(&ctx, item).await {
                Ok(state) => Command::message(Messages::RaisePopup(Box::new(state))),
                Err(e) => {
                    let e = anyhow!("Failed to load folders: {e}");
                    tracing::error!("{e:?}");
                    Command::message(e)
                }
            }
        })
    }

    fn open_label_popup(&mut self, item: Items) -> Command<Messages> {
        if matches!(&self.state, State::Syncing(_)) {
            return Command::None;
        }

        let ctx = Arc::clone(&self.ctx);
        Command::task(async move {
            match LabelItemPopup::new(&ctx, item).await {
                Ok(state) => Command::message(Messages::RaisePopup(Box::new(state))),
                Err(e) => {
                    let e = anyhow!("Failed to load labels: {e}");
                    tracing::error!("{e:?}");
                    Command::message(e)
                }
            }
        })
    }

    fn open_contacts(&mut self) -> Command<Messages> {
        Command::message(Messages::SwitchAppState(AppState::Contacts(
            crate::app_model::contacts::ContactsModel::new(self.ctx.clone()),
        )))
    }

    fn select_label(&mut self, label_id: LocalLabelId) -> Command<Messages> {
        let ctx = Arc::clone(&self.ctx);
        Command::task(async move {
            let stash = ctx.user_stash();
            Command::message(
                match async {
                    let tether = stash.connection().await?;
                    Mailbox::new(&tether, label_id).await
                }
                .await
                {
                    Ok(mbox) => Message::Sync(mbox).into(),
                    Err(e) => {
                        let e = anyhow!("Failed to open label: {e}");
                        tracing::error!("{e:?}");
                        Messages::DisplayError(None, e)
                    }
                },
            )
        })
    }

    fn change_filter(&mut self, filter: ReadFilter) {
        self.filter = filter;
        if let State::Conversations(state) = &mut self.state {
            let _ = state.paginator().clone_inner().change_filter(filter);
        } else if let State::Messages(state) = &mut self.state {
            state
                .label_paginator()
                .map(|paginator| paginator.clone_inner().change_filter(filter));
        }
    }

    fn scroller_fetch_new(&mut self) {
        debug!("scrolling fetch_new");
        if let State::Conversations(state) = &mut self.state {
            let _ = state.paginator().clone_inner().fetch_new();
        } else if let State::Messages(state) = &mut self.state {
            let _ = state
                .label_paginator()
                .map(|paginator| paginator.clone_inner().fetch_new());
        }
    }

    fn clear_cursor(&mut self) {
        if let State::Conversations(state) = &mut self.state {
            let _ = state.paginator().clone_inner().clear_cursor();
        } else if let State::Messages(state) = &mut self.state {
            state
                .label_paginator()
                .map(|paginator| paginator.clone_inner().clear_cursor());
        }
    }
}

impl AppStateHandler for MailboxModel {
    fn on_state_enter(&mut self) -> Command<Messages> {
        Command::batch([
            self.create_background_worker(),
            Command::message(Message::Sync(self.mailbox.clone())),
            Command::background_task({
                let ctx = Arc::clone(&self.ctx);
                let tok = self.cancel_token.clone();
                move |sender| observe_draft_action_errors(ctx, tok, sender).boxed()
            }),
            Command::background_task({
                let ctx = Arc::clone(&self.ctx);
                let tok = self.cancel_token.clone();
                move |sender| observe_event_loop_errors(ctx, tok, sender).boxed()
            }),
        ])
    }

    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        if let Some(composer) = &mut self.composer {
            return composer.handle_event(&self.ctx, &self.mailbox, event);
        }

        if let Some(search) = &mut self.search {
            return search.handle_event(&event);
        }

        if let Event::Key(key) = &event {
            match key.code {
                KeyCode::Char('h') if key.modifiers.is_empty() => {
                    return Command::Message(Messages::RaisePopup(Box::new(HelpPopup {
                        items: self.help_options(),
                        list_state: ScrollableListState::new(Some(0)),
                    })));
                }
                KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Composer::empty(Arc::clone(&self.ctx));
                }
                KeyCode::Char('c') => {
                    return Command::message(Message::OpenContacts);
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.change_filter(ReadFilter::Unread);
                    return Command::None;
                }
                KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.change_filter(ReadFilter::Read);
                    return Command::None;
                }
                KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.change_filter(ReadFilter::All);
                    return Command::None;
                }
                KeyCode::F(8) => {
                    return Command::Message(Messages::SwitchAppState(AppState::Background(
                        crate::app_model::background::BackgroundModel::new(
                            self.ctx.clone(),
                            Box::new(|ctx| {
                                Command::batch([
                                    Command::message(Messages::DisplayBackgroundProgress(
                                        "Loading mailbox ...".to_owned(),
                                    )),
                                    Command::task(async move {
                                        let model = MailboxModel::new(ctx).await;
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
                            }),
                        ),
                    )));
                }
                KeyCode::F(4) if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.scroller_fetch_new();
                    return Command::None;
                }
                KeyCode::F(4) => {
                    self.clear_cursor();
                    return Command::None;
                }
                KeyCode::F(5) if key.modifiers.contains(KeyModifiers::SHIFT) => {
                    return refresh(self.ctx.as_arc());
                }
                KeyCode::F(5) => {
                    return poll_event_loop(self.ctx.as_arc());
                }
                _ => (),
            }
        }

        if let Event::Key(key) = &event
            && key.code == KeyCode::Char('/')
        {
            return Command::Message(Message::SearchPopup(Search::new()).into());
        }

        match &mut self.state {
            State::Syncing(_) => Command::None,
            State::Conversations(state) => state.handle_event(&self.ctx, &self.mailbox, &event),
            State::Messages(state) => state.handle_event(&self.ctx, &self.mailbox, &event),
        }
    }

    fn update(&mut self, _: &Arc<MailContext>, message: Messages) -> Command<Messages> {
        let Messages::Mailbox(message) = message else {
            return Command::None;
        };

        if matches!(&message, Message::CloseComposer) {
            self.composer = None;
            return Command::None;
        }

        if let Some(composer) = &mut self.composer {
            return composer.update(&self.ctx, message);
        }

        if self.search.is_some() {
            let Message::CloseSearchPopup = message else {
                return Search::update(&self.ctx, message, &self.mailbox);
            };
        }

        match message {
            Message::Sync(mbox) => self.sync_mailbox(mbox),
            Message::OpenConversationView(mbox, label, state) => {
                self.open_conversation_view(mbox, label, state)
            }
            Message::OpenMessageView(mbox, label, state) => {
                self.open_message_view(mbox, label, state)
            }
            Message::OpenSearchView(mbox, state) => self.open_search_view(mbox, state),
            Message::OpenLabelSelectPopup => self.open_label_select_popup(),
            Message::SelectLabel(label_id) => self.select_label(label_id),
            Message::OpenMoveItemsPopup(item) => self.open_move_item_popup(item),
            Message::OpenLabelItemPopup(item) => self.open_label_popup(item),
            Message::OpenSnoozePopup(item) => {
                SnoozeItemPopup::new(&self.ctx, item, self.label.local_id.unwrap())
            }
            Message::OpenCustomSnoozePopup(item, local_label_id) => {
                CustomSnoozeOption::new(item, local_label_id)
            }
            Message::ConversationState(_) | Message::MessageState(_) => {
                self.state.update(&self.ctx, message, &self.mailbox)
            }
            Message::LabelRefreshed(label) => {
                self.label = label;
                Command::None
            }
            Message::OpenComposer(composer) => {
                self.composer = Some(composer);
                Command::None
            }
            Message::CloseComposer => {
                self.composer = None;
                Command::None
            }
            Message::NewLabelWatcher(handle) => {
                self.label_watcher = Some(handle);
                Command::None
            }
            Message::OpenContacts => self.open_contacts(),
            Message::SearchPopup(search) => {
                self.search = Some(search);
                Command::None
            }
            Message::CloseSearchPopup => {
                self.search = None;
                Command::None
            }
            Message::SearchStatusBar(status) => {
                self.search_status = Some(status);
                Command::None
            }
            Message::ClearSearchStatusBar => {
                self.search_status = None;
                Command::None
            }
            Message::Composer(_) | Message::SearchSubmit(_) => Command::None,
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        self.state.view(frame, area);
        if let Some(composer) = &mut self.composer {
            composer.view(frame, area);
        }
        if let Some(search) = &mut self.search {
            search.view(frame, area);
        }
    }

    fn help_options(&self) -> Vec<(&'static str, &'static str)> {
        let mut items = vec![
            ("k, ▲", "Go up"),
            ("j, ▼", "Go down"),
            ("[space]", "toggle selection of current item"),
            ("g/G", "Select/Deselect all loaded items"),
            ("Tab", "Toggle"),
            ("s", "Select a label or a folder"),
            ("m", "Move the selected item"),
            ("r", "Mark a message as read"),
            ("u", "Mark a message as unread"),
            ("f/F", "Star/Unstar the selected item"),
            ("l", "Label a message"),
            ("d", "Delete a message permanently"),
            ("Ctrl + n", "Create a new message"),
            ("Ctrl + u", "Show only unread messages"),
            ("Ctrl + t", "Show only read messages"),
            ("Ctrl + a", "Show all messages"),
            ("/", "Open the search bar"),
            ("C", "Show the contact list"),
            ("F4", "Clear cache"),
            ("Shift+F5", "Reload all data from server"),
            ("F5", "Refresh (Force event loop poll)"),
            ("F8", "[DEBUG]: Put app in background"),
            ("ctrl+h", "[DEBUG]: has more?"),
        ];

        self.state.help_options(&mut items);
        if self.composer.is_some() {
            Composer::help_options(&mut items);
        }
        if self.search.is_some() {
            Search::help_options(&mut items);
        }

        info!(?items);

        items
    }

    fn help_bar_lines(&self) -> u16 {
        1
    }
    fn view_help_bar(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(Line::from("Press F1/h to display the help popup"), area);
    }

    fn view_status_bar(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(ref status) = self.search_status {
            let [count_area, _, other_area] = Layout::horizontal([
                Constraint::Length(16),
                Constraint::Length(1),
                Constraint::Percentage(100),
            ])
            .flex(Flex::Start)
            .areas(area);

            let count = format!("Search T:{total:4}", total = status.total);
            frame.render_widget(Text::from(count), count_area);

            let search = format!("phrase: `{}`, press ESC to go back.", status.search_phrase);
            frame.render_widget(Text::from(search), other_area);
        } else {
            let label_name = self
                .label
                .path
                .as_deref()
                .unwrap_or(self.label.name.as_str());

            let (total, unread) = if self.mailbox.view_mode() == ViewMode::Conversations {
                (self.label.total_conv, self.label.unread_conv)
            } else {
                (self.label.total_msg, self.label.unread_msg)
            };
            let counters = format!("T:{total:4} U:{unread:4}");
            let [label_area, _, count_area, filter_area, other_area] = Layout::horizontal([
                Constraint::Length(u16::try_from(label_name.chars().count()).unwrap_or(10)),
                Constraint::Length(1),
                Constraint::Length(13),
                Constraint::Length(13),
                Constraint::Percentage(100),
            ])
            .flex(Flex::Start)
            .areas(area);

            let text = Text::from(label_name);
            frame.render_widget(text, label_area);
            frame.render_widget(Text::from(counters), count_area);
            frame.render_widget(
                Text::from(format!(" | {:?} | ", self.filter).bold()),
                filter_area,
            );
            if let State::Conversations(state) = &mut self.state {
                state.draw_status_bar(frame, other_area);
            }
        }
    }
}

impl State {
    fn update(
        &mut self,
        user_ctx: &Arc<MailUserContext>,
        message: Message,
        mbox: &Mailbox,
    ) -> Command<Messages> {
        match self {
            State::Syncing(_) => Command::None,
            State::Conversations(state) => state.update(user_ctx, message, mbox),
            State::Messages(state) => state.update(user_ctx, message, mbox),
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        match self {
            State::Syncing(state) => {
                let throbber = throbber_widgets_tui::Throbber::default()
                    .label("Loading...")
                    .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
                    .use_type(throbber_widgets_tui::WhichUse::Spin);
                frame.render_stateful_widget(CenteredThrobber::new(throbber), area, state);
            }
            State::Conversations(state) => {
                state.view(frame, area);
            }
            State::Messages(state) => {
                state.view(frame, area);
            }
        }
    }

    pub fn help_options(&self, vec: &mut Vec<(&'static str, &'static str)>) {
        match self {
            State::Syncing(_) => (),
            State::Conversations(s) => s.help_options(vec),
            State::Messages(s) => s.help_options(vec),
        }
    }
}

impl From<MailboxModel> for AppState {
    fn from(value: MailboxModel) -> Self {
        Self::Mailbox(value)
    }
}

fn background_worker(
    context: Arc<MailUserContext>,
    cancel_token: CancellationToken,
) -> Command<Messages> {
    Command::background_task(|sender| {
        async move {
            let mut observer = ActionFailureObserver::<EventPoll>::new(context.action_queue());
            loop {
                tokio::select! {
                () = cancel_token.cancelled() => {
                    return;
                }
                r = observer.next() => {
                        if let Ok(ActionFailureReason::Error(err, _)) = r {
                            let e = anyhow!("Failed to poll events: {err:?}");
                            if sender
                                .send(Command::message(Messages::DisplayError(
                                    Some("Event Loop".to_owned()),
                                    e,
                                )))
                                .is_err()
                            {
                                error!("Failed to send message from worker");
                            }
                        }
                    }
                }
            }
        }
        .boxed()
    })
}

/// Observe and report draft save failures.
async fn observe_draft_action_errors(
    ctx: Arc<MailUserContext>,
    cancellation_token: CancellationToken,
    sender: Sender<Command<Messages>>,
) {
    let mut observer = match DraftSendResultWatcher::new(
        ctx.user_stash().clone(),
        DraftSendResultWatcherMode::SentOnly,
    )
    .await
    {
        Ok(observer) => observer,
        Err(e) => {
            error!("Failed to create draft send result observer:{e:?}");
            let _ = sender
                .send_async(Command::message(Messages::DisplayError(
                    Some("Draft Send Result".to_owned()),
                    anyhow::Error::new(e),
                )))
                .await;
            return;
        }
    };

    loop {
        tokio::select! {
            () = cancellation_token.cancelled() => {
                debug!(
                    "Exiting draft save observer"
                );
                return;
            }
            r = observer.next() => {
                match r {
                    Ok(failures) => {
                        handle_draft_failure(&ctx, &sender, failures).await;
                    }
                    Err(e) => {
                        error!("Failed to observe: {e:?}");
                        return;
                    }
                }
            }

        }
    }
}

async fn handle_draft_failure(
    ctx: &Arc<MailUserContext>,
    sender: &Sender<Command<Messages>>,
    results: Vec<DraftSendResult>,
) {
    for result in results {
        if result.is_success() {
            if result.origin == DraftSendResultOrigin::Send && result.is_send_undoable() {
                let ctx = Arc::clone(ctx);
                let popup = YesNoPopup::new(
                    "Undo Send?",
                    "Message was sent successfully, would you like to undo this send?",
                )
                .on_accept(Command::batch([
                    Command::message(Messages::DisplayBackgroundProgress(
                        "Cancelling Send".to_owned(),
                    )),
                    Command::task(async move {
                        let result_cmd = match Draft::action_undo_send(
                            ctx.action_queue(),
                            result.local_message_id,
                        )
                        .await
                        {
                            // On success open composer, else display error
                            Ok(_) => Composer::open(Arc::clone(&ctx), result.local_message_id),
                            Err(e) => Command::message(Messages::DisplayError(
                                Some("Undo Send Error".to_owned()),
                                anyhow::Error::new(e),
                            )),
                        };
                        Command::batch([
                            Command::message(Messages::DismissBackgroundProgress),
                            result_cmd,
                        ])
                    }),
                ]));

                let _ = sender
                    .send_async(Command::message(Messages::raise_popup(popup)))
                    .await;
            } else if result.origin == DraftSendResultOrigin::ScheduleSend {
                let dt = date_from_timestamp(result.undo_timestamp);
                let _ = sender
                    .send_async(Command::message(Messages::DisplayInfo(
                        Some("Message Schedule Send Success".to_owned()),
                        format!("Message will be delivered at {dt}"),
                    )))
                    .await;
            }

            continue;
        }

        if result.origin == DraftSendResultOrigin::Save {
            let _ = sender
                .send_async(Command::message(Messages::DisplayError(
                    Some("Failed to Save Draft".to_string()),
                    anyhow!("{:?}", result.error.unwrap_or(DraftSendFailure::Internal)),
                )))
                .await;
        } else {
            let err_command = Command::message(Messages::DisplayError(
                Some("Failed to Send Draft".to_string()),
                anyhow!("{:?}", result.error.unwrap_or(DraftSendFailure::Internal)),
            ));

            let open_composer_command = Composer::open(Arc::clone(ctx), result.local_message_id);
            let _ = sender
                .send_async(Command::batch([err_command, open_composer_command]))
                .await;
        }
    }
}

/// Observe event loop errors
async fn observe_event_loop_errors(
    ctx: Arc<MailUserContext>,
    cancellation_token: CancellationToken,
    sender: Sender<Command<Messages>>,
) {
    let mut oberserver = ActionFailureObserver::<EventPoll>::new(ctx.action_queue());

    loop {
        select! {
            () = cancellation_token.cancelled() => {
                return;
            }

            r = oberserver.next() => {
                let Ok(v) = r else {
                    return;
                };
                handle_event_loop_error(v, &sender).await;
            }
        }
    }
}

// Convert the error into a user displayable message.
async fn handle_event_loop_error(error: ActionFailureReason, sender: &Sender<Command<Messages>>) {
    if let ActionFailureReason::Error(e, _) = error {
        let cmd = if let Some(details) = e.as_action_error::<EventPoll>() {
            match details {
                ActionError::Action(e) => Command::message(Messages::DisplayError(
                    Some("Event Loop".to_owned()),
                    anyhow!("Event Poll Failure: {}", e),
                )),
                ActionError::Queue(e) => Command::message(Messages::DisplayError(
                    Some("Event Loop".to_owned()),
                    anyhow!("Queue Failure: {}", e),
                )),
            }
        } else {
            Command::message(Messages::DisplayError(
                Some("Event Loop".to_owned()),
                anyhow!("Failed to poll the event loop"),
            ))
        };
        let _ = sender.send_async(cmd).await;
    }
}
