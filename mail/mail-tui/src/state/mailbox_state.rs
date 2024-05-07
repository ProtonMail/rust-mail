use crate::app::{AppBackgroundDispatcher, AppLocalDispatcher};
use crate::events::mailbox::MailboxEvent;
use crate::events::AppEvent;
use crate::state::AppState;
use crate::views::{ConversationView, SessionsView};
use proton_async::runtime::MultiThreaded;
use proton_async::sync::mpsc::Sender;
use proton_core_common::db::proton_sqlite3::{InProcessTrackerService, Live, LiveQueryBuilder};
use proton_mail_common::db::proton_sqlite3::Observable;
use proton_mail_common::db::{
    ConversationQuery, LabelsByTypeQueryWithConversationCount, LocalLabelId, MessageQuery,
};
use proton_mail_common::exports::tracing::warn;
use proton_mail_common::proton_api_mail::domain::{LabelId, MailSettingsViewMode};
use proton_mail_common::proton_api_mail::proton_api_core::exports::tracing::{debug, error};
use proton_mail_common::{
    MailContext, MailContextError, MailUserContext, MailUserContextInitializationCallback,
    MailUserContextLoadingStage, Mailbox, MailboxError,
};

const CONVERSATION_COUNT: usize = 50;
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum LoadingState {
    Unloaded,
    Loading,
    Done,
}

#[derive(Debug, thiserror::Error)]
pub enum MailboxStateError {
    #[error("{0}")]
    Mailbox(
        #[source]
        #[from]
        MailboxError,
    ),
    #[error("{0}")]
    MailboxContext(
        #[source]
        #[from]
        MailContextError,
    ),
    #[error("No user sessions available")]
    NoContext,
}

enum BackgroundTask {
    EventPoll,
    FlushQueue,
}

pub struct MailboxState {
    user_context: Option<MailboxUserContextState>,
    labels_loading_state: LoadingState,
    conversation_loading_state: LoadingState,
    pending_context: Option<MailUserContext>,
}

pub enum ViewMode {
    Conversations(Live<ConversationQuery>),
    Messages(Live<MessageQuery>),
}

impl ViewMode {
    pub fn new(mbox: &Mailbox) -> Result<Self, MailboxError> {
        Ok(if mbox.view_mode() == MailSettingsViewMode::Conversations {
            ViewMode::Conversations(
                mbox.new_conversation_query(new_live_query, CONVERSATION_COUNT)?,
            )
        } else {
            ViewMode::Messages(mbox.new_messages_query(new_live_query, CONVERSATION_COUNT)?)
        })
    }
}
pub struct MailboxUserContextState {
    pub mailbox: Mailbox,
    pub view_mode: ViewMode,
    pub system_labels: Live<LabelsByTypeQueryWithConversationCount>,
    pub folders: Live<LabelsByTypeQueryWithConversationCount>,
    pub labels: Live<LabelsByTypeQueryWithConversationCount>,
    event_loop_poller: Sender<BackgroundTask>,
}

impl MailboxUserContextState {
    fn new(
        mailbox: Mailbox,
        mail_user_context: &MailUserContext,
        event_loop_poller: Sender<BackgroundTask>,
    ) -> Result<Self, MailboxError> {
        Ok(Self {
            view_mode: ViewMode::new(&mailbox)?,
            system_labels: mail_user_context.new_system_labels_live_query(new_live_query),
            folders: mail_user_context.new_folder_labels_live_query(new_live_query),
            labels: mail_user_context.new_label_labels_live_query(new_live_query),
            mailbox,
            event_loop_poller,
        })
    }
}

impl MailboxState {
    pub fn new() -> Self {
        Self {
            user_context: None,
            labels_loading_state: LoadingState::Unloaded,
            conversation_loading_state: LoadingState::Unloaded,
            pending_context: None,
        }
    }

    pub fn labels_loading_state(&self) -> LoadingState {
        self.labels_loading_state
    }

    pub fn conversation_loading_state(&self) -> LoadingState {
        self.conversation_loading_state
    }

    pub fn active_label(&self) -> Option<LocalLabelId> {
        self.user_context.as_ref().map(|c| c.mailbox.label_id())
    }

    pub fn mailbox_context(&self) -> Option<&MailboxUserContextState> {
        self.user_context.as_ref()
    }

    fn on_refresh(&mut self, mut app_dispatcher: AppLocalDispatcher<AppState, AppEvent>) {
        let Some(user_context) = &self.user_context else {
            app_dispatcher.set_error("Mailbox Error", MailboxStateError::NoContext);
            return;
        };

        if let Err(e) = user_context
            .mailbox
            .refresh(Box::new(MailboxInitCallback::for_refresh(
                app_dispatcher.background_dispatcher(),
            )))
        {
            app_dispatcher.set_error("Refresh Failed", e);
            return;
        }

        self.labels_loading_state = LoadingState::Loading;
        self.conversation_loading_state = LoadingState::Loading;
    }

    fn on_mailbox_initialized(
        &mut self,
        mut app_dispatcher: AppLocalDispatcher<AppState, AppEvent>,
    ) {
        let Some(user_context) = self.pending_context.take() else {
            app_dispatcher.set_error("Mailbox Error", MailboxStateError::NoContext);
            return;
        };

        self.conversation_loading_state = LoadingState::Done;

        let mailbox = match Mailbox::with_remote_id(user_context.clone(), LabelId::inbox()) {
            Ok(m) => m,
            Err(e) => {
                app_dispatcher.set_error("Failed to Open Mailbox", e);
                return;
            }
        };

        let bg_dispatcher = app_dispatcher.background_dispatcher();
        user_context.mail_context().async_runtime().block_on(async {
            if let Err(e) = mailbox.sync(CONVERSATION_COUNT).await {
                bg_dispatcher.set_error("Mailbox Error", e)
            }
        });

        let ctx = mailbox.user_context().clone();
        let dispatcher = app_dispatcher.background_dispatcher();
        let (s, r) = proton_async::sync::mpsc::unbounded();
        let ctx_cloned = ctx.clone();

        std::thread::spawn(move || loop {
            let Ok(task) = r.recv() else {
                return;
            };
            match task {
                BackgroundTask::FlushQueue => {
                    debug!("Executing Queue");
                    if let Err(e) = ctx.execute_pending_actions() {
                        dispatcher.set_error("Queue Error", e);
                    }
                }

                BackgroundTask::EventPoll => {
                    debug!("Polling Events");
                    let ctx_cloned = ctx.clone();
                    if let Err(e) = ctx
                        .mail_context()
                        .async_runtime()
                        .block_on(async { ctx_cloned.poll_event_loop().await })
                    {
                        dispatcher.set_error("Event Loop Error", e);
                    }
                }
            }
        });

        match MailboxUserContextState::new(mailbox, &ctx_cloned, s) {
            Ok(m) => self.user_context = Some(m),
            Err(e) => {
                app_dispatcher.set_error("Mailbox Error", e);
            }
        }
    }

    fn on_mailbox_open(
        &mut self,
        app_dispatcher: AppLocalDispatcher<AppState, AppEvent>,
        user_context: MailUserContext,
    ) {
        self.labels_loading_state = LoadingState::Loading;
        self.conversation_loading_state = LoadingState::Loading;
        user_context.initialize(
            LabelId::inbox().clone(),
            Box::new(MailboxInitCallback::for_init(
                app_dispatcher.background_dispatcher(),
            )),
        );
        self.pending_context = Some(user_context);
    }
    pub fn load_label(
        &mut self,
        runtime: &MultiThreaded,
        label_id: LocalLabelId,
        mut dispatcher: AppLocalDispatcher<AppState, AppEvent>,
    ) {
        let Some(mailbox_context) = &mut self.user_context else {
            dispatcher.set_error("Mailbox Error", MailboxStateError::NoContext);
            return;
        };

        let background_dispatcher = dispatcher.background_dispatcher();
        mailbox_context.mailbox =
            match Mailbox::with_id(mailbox_context.mailbox.user_context().clone(), label_id) {
                Ok(m) => m,
                Err(e) => {
                    dispatcher.set_error("Mailbox Error", e);
                    return;
                }
            };
        let mbox = mailbox_context.mailbox.clone();
        runtime.spawn(async move {
            let r = mbox.sync(CONVERSATION_COUNT).await;
            background_dispatcher
                .queue_event(MailboxEvent::LoadConversations(r.map_err(Into::into)));
        });
        match ViewMode::new(&mailbox_context.mailbox) {
            Ok(m) => {
                mailbox_context.view_mode = m;
                self.conversation_loading_state = LoadingState::Loading;
            }
            Err(e) => {
                dispatcher.set_error("Mailbox Error", e);
            }
        }
    }

    pub fn poll_event_loop(&mut self) {
        debug!("Submitting event loop poll");
        let Some(ctx) = &mut self.user_context else {
            return;
        };
        if ctx
            .event_loop_poller
            .send(BackgroundTask::EventPoll)
            .is_err()
        {
            error!("Could not event poll request")
        }
    }

    pub fn exec_queue(&mut self) {
        debug!("Submitting queue exec");
        let Some(ctx) = &mut self.user_context else {
            return;
        };
        if ctx
            .event_loop_poller
            .send(BackgroundTask::FlushQueue)
            .is_err()
        {
            error!("Could not queue exec request")
        }
    }

    pub fn handle_event(
        &mut self,
        mut dispatcher: AppLocalDispatcher<AppState, AppEvent>,
        event: MailboxEvent,
        ctx: &MailContext,
    ) {
        match event {
            MailboxEvent::LoadLabelRequest(label_id) => {
                self.load_label(ctx.async_runtime(), label_id, dispatcher);
            }
            MailboxEvent::MailboxRefresh => {
                self.on_refresh(dispatcher);
            }
            MailboxEvent::NewMailboxSession(context) => {
                dispatcher.push_view(ConversationView::new());
                self.on_mailbox_open(dispatcher, context);
            }
            MailboxEvent::NewMailboxSessionInitialized => {
                self.on_mailbox_initialized(dispatcher);
            }
            MailboxEvent::LoadLabels(r) => match r {
                Ok(_) => {
                    self.labels_loading_state = LoadingState::Done;
                }
                Err(e) => {
                    dispatcher.set_error("Failed to load labels", e);
                }
            },
            MailboxEvent::LoadConversations(result) => match result {
                Ok(_) => {
                    self.conversation_loading_state = LoadingState::Done;
                }
                Err(e) => {
                    self.conversation_loading_state = LoadingState::Done;
                    dispatcher.set_error("Failed to load conversations", e);
                }
            },
            MailboxEvent::Logout => {
                if let Some(mailbox_context) = self.user_context.take() {
                    let bg_dispatcher = dispatcher.background_dispatcher();
                    let ctx = mailbox_context.mailbox.user_context().clone();
                    mailbox_context
                        .mailbox
                        .user_context()
                        .mail_context()
                        .async_runtime()
                        .spawn(async move {
                            if let Err(e) = ctx.logout().await {
                                bg_dispatcher.set_error("Failed to Logout", e);
                                return;
                            }
                            bg_dispatcher.queue_on_main(|app| {
                                app.pop_all_views();
                                app.push_view(SessionsView::new());
                            });
                        });
                }
            }
            MailboxEvent::PollEventLoop => {
                self.poll_event_loop();
            }
            MailboxEvent::ExecQueue => {
                self.exec_queue();
            }
            MailboxEvent::DeleteConversation(id) => {
                if let Some(mailbox_context) = &self.user_context {
                    if let Err(e) = mailbox_context
                        .mailbox
                        .delete_conversations(std::iter::once(id))
                    {
                        dispatcher.set_error("Failed to delete conversation", e);
                    }
                } else {
                    warn!("No user context for delete conversation")
                }
            }
            MailboxEvent::MarkConversationRead(id) => {
                if let Some(mailbox_context) = &self.user_context {
                    if let Err(e) = mailbox_context
                        .mailbox
                        .mark_conversations_read(std::iter::once(id))
                    {
                        dispatcher.set_error("Failed to mark conversation read", e);
                    }
                } else {
                    warn!("No user context for mark conversation read")
                }
            }
            MailboxEvent::MarkConversationUnread(id) => {
                if let Some(mailbox_context) = &self.user_context {
                    if let Err(e) = mailbox_context
                        .mailbox
                        .mark_conversations_unread(std::iter::once(id))
                    {
                        dispatcher.set_error("Failed to mark conversation unread", e);
                    }
                } else {
                    warn!("No user context for mark conversation unread")
                }
            }
            MailboxEvent::LabelConversation(id, label_id) => {
                if let Some(mailbox_context) = &self.user_context {
                    if let Err(e) = mailbox_context
                        .mailbox
                        .label_conversations(label_id, std::iter::once(id))
                    {
                        dispatcher.set_error("Failed to label conversation", e);
                    }
                } else {
                    warn!("No user context for label conversation")
                }
            }
            MailboxEvent::UnlabelConversation(id, label_id) => {
                if let Some(mailbox_context) = &self.user_context {
                    if let Err(e) = mailbox_context
                        .mailbox
                        .unlabel_conversations(label_id, std::iter::once(id))
                    {
                        dispatcher.set_error("Failed to unlabel conversation", e);
                    }
                } else {
                    warn!("No user context for unlabel conversation")
                }
            }
            MailboxEvent::MoveConversation(id, label_id) => {
                if let Some(mailbox_context) = &self.user_context {
                    if let Err(e) = mailbox_context
                        .mailbox
                        .move_conversations(label_id, std::iter::once(id))
                    {
                        dispatcher.set_error("Failed to move conversation", e);
                    }
                } else {
                    warn!("No user context for move conversation")
                }
            }
        }
    }
}

struct MailboxInitCallback(AppBackgroundDispatcher<AppState, AppEvent>, bool);

impl MailboxInitCallback {
    fn for_init(dispatcher: AppBackgroundDispatcher<AppState, AppEvent>) -> Self {
        Self(dispatcher, true)
    }

    fn for_refresh(dispatcher: AppBackgroundDispatcher<AppState, AppEvent>) -> Self {
        Self(dispatcher, false)
    }
}
impl MailUserContextInitializationCallback for MailboxInitCallback {
    fn on_stage(&self, stage: MailUserContextLoadingStage) {
        if stage == MailUserContextLoadingStage::Finished {
            self.0.queue_event(MailboxEvent::LoadLabels(Ok(())));
            if self.1 {
                self.0
                    .queue_event(MailboxEvent::NewMailboxSessionInitialized);
            } else {
                self.0.queue_event(MailboxEvent::LoadConversations(Ok(())))
            }
        }
    }

    fn on_stage_err(&self, stage: MailUserContextLoadingStage, err: MailContextError) {
        match stage {
            MailUserContextLoadingStage::Labels => {
                self.0
                    .queue_event(MailboxEvent::LoadLabels(Err(err.into())));
            }
            _ => {
                self.0.set_error("Failed to load", err);
            }
        }
    }
}

fn new_live_query<Q: Observable>(tracker: InProcessTrackerService, query: Q) -> Live<Q> {
    LiveQueryBuilder::new(tracker)
        .with_foreground_initializer()
        .build(query)
}
