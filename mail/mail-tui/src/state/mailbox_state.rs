use crate::app::{AppBackgroundDispatcher, AppLocalDispatcher};
use crate::events::mailbox::MailboxEvent;
use crate::events::AppEvent;
use crate::state::AppState;
use crate::views::{ConversationView, SessionsView};
use proton_async::sync::mpsc::Sender;
use proton_core_common::proton_core_db::proton_sqlite3::{
    InProcessTrackerService, LiveQuery, LiveQueryBuilder,
};
use proton_mail_common::proton_api_mail::domain::LabelId;
use proton_mail_common::proton_api_mail::proton_api_core::exports::tracing::{debug, error};
use proton_mail_common::proton_mail_db::proton_sqlite3::ObservableQuery;
use proton_mail_common::proton_mail_db::{
    ConversationQuery, LabelsByTypeQueryWithConversationCount, LocalLabelId,
};
use proton_mail_common::{
    MailContextError, MailUserContext, MailUserContextInitializationCallback,
    MailUserContextLoadingStage, Mailbox, MailboxError, MailboxResult,
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

pub struct MailboxState {
    user_context: Option<MailboxUserContextState>,
    labels_loading_state: LoadingState,
    conversation_loading_state: LoadingState,
    pending_context: Option<MailUserContext>,
}

pub struct MailboxUserContextState {
    mailbox: Mailbox,
    pub conversations: LiveQuery<ConversationQuery>,
    pub system_labels: LiveQuery<LabelsByTypeQueryWithConversationCount>,
    pub folders: LiveQuery<LabelsByTypeQueryWithConversationCount>,
    pub labels: LiveQuery<LabelsByTypeQueryWithConversationCount>,
    event_loop_poller: Sender<()>,
}

impl MailboxUserContextState {
    pub fn new(mailbox: Mailbox, event_loop_poller: Sender<()>) -> Self {
        Self {
            conversations: mailbox.new_conversation_query(new_live_query, CONVERSATION_COUNT),
            system_labels: mailbox
                .user_context()
                .new_system_labels_live_query(new_live_query),
            folders: mailbox
                .user_context()
                .new_folder_labels_live_query(new_live_query),
            labels: mailbox
                .user_context()
                .new_label_labels_live_query(new_live_query),
            mailbox,
            event_loop_poller,
        }
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

        let mailbox = match Mailbox::with_remote_id(user_context, LabelId::inbox()) {
            Ok(m) => m,
            Err(e) => {
                app_dispatcher.set_error("Failed to Open Mailbox", e);
                return;
            }
        };

        let ctx = mailbox.user_context().clone();
        let dispatcher = app_dispatcher.background_dispatcher();
        let (s, r) = proton_async::sync::mpsc::unbounded();

        mailbox
            .user_context()
            .mail_context()
            .async_runtime()
            .spawn(async move {
                loop {
                    if r.recv_async().await.is_err() {
                        return;
                    }
                    debug!("Polling Event Loop");
                    if let Err(e) = ctx.poll_event_loop().await {
                        dispatcher.set_error("Event Loop", e);
                    }
                }
            });
        self.user_context = Some(MailboxUserContextState::new(mailbox, s));
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
        label_id: LocalLabelId,
        mut dispatcher: AppLocalDispatcher<AppState, AppEvent>,
    ) {
        let Some(mailbox_context) = &mut self.user_context else {
            dispatcher.set_error("Mailbox Error", MailboxStateError::NoContext);
            return;
        };

        let background_dispatcher = dispatcher.background_dispatcher();
        mailbox_context.mailbox =
            Mailbox::with_id(mailbox_context.mailbox.user_context().clone(), label_id);
        if let Err(e) = mailbox_context.mailbox.sync(
            CONVERSATION_COUNT,
            Some(Box::new(move |r: MailboxResult<()>| {
                background_dispatcher
                    .queue_event(MailboxEvent::LoadConversations(r.map_err(|e| e.into())))
            })),
        ) {
            dispatcher.set_error("Mailbox Error", e);
            return;
        }
        mailbox_context.conversations = mailbox_context
            .mailbox
            .new_conversation_query(new_live_query, CONVERSATION_COUNT);
        self.conversation_loading_state = LoadingState::Loading;
    }

    pub fn poll_event_loop(&mut self) {
        let Some(ctx) = &mut self.user_context else {
            return;
        };
        if ctx.event_loop_poller.send(()).is_err() {
            error!("Could not send poll request")
        }
    }

    pub fn handle_event(
        &mut self,
        mut dispatcher: AppLocalDispatcher<AppState, AppEvent>,
        event: MailboxEvent,
    ) {
        match event {
            MailboxEvent::LoadLabelRequest(label_id) => {
                self.load_label(label_id, dispatcher);
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
        match stage {
            MailUserContextLoadingStage::Conversation => {
                self.0.queue_event(MailboxEvent::LoadLabels(Ok(())))
            }
            MailUserContextLoadingStage::Finished => {
                if self.1 {
                    self.0
                        .queue_event(MailboxEvent::NewMailboxSessionInitialized);
                } else {
                    self.0.queue_event(MailboxEvent::LoadConversations(Ok(())))
                }
            }
            _ => {}
        }
    }

    fn on_stage_err(&self, stage: MailUserContextLoadingStage, err: MailContextError) {
        match stage {
            MailUserContextLoadingStage::Labels => {
                self.0
                    .queue_event(MailboxEvent::LoadLabels(Err(err.into())));
            }
            MailUserContextLoadingStage::Conversation => {
                self.0
                    .queue_event(MailboxEvent::LoadConversations(Err(err.into())));
            }
            _ => {
                self.0.set_error("Failed to load", err);
            }
        }
    }
}

fn new_live_query<Q: ObservableQuery>(tracker: InProcessTrackerService, query: Q) -> LiveQuery<Q> {
    LiveQueryBuilder::new(tracker)
        .with_foreground_initializer()
        .build(query)
}
