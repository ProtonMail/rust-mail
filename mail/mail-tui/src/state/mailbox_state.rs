use crate::app::{AppBackgroundDispatcher, AppLocalDispatcher};
use crate::events::mailbox::MailboxEvent;
use crate::events::AppEvent;
use crate::state::AppState;
use crate::views::{ConversationView, SessionsView};
use proton_mail_common::proton_api_mail::domain::{LabelType, SysLabelId};
use proton_mail_common::proton_api_mail::proton_api_core::exports::tracing;
use proton_mail_common::proton_mail_db::{
    ConversationsLiveQuery, LabelsByTypeWithConversationCountLiveQuery, LocalLabel, LocalLabelId,
};
use proton_mail_common::{
    MailContext, MailContextError, MailUserContext, MailUserContextInitializationCallback,
    MailUserContextLoadingStage,
};

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum LoadingState {
    Unloaded,
    Loading,
    Done,
}

#[derive(Debug, thiserror::Error)]
pub enum MailboxStateError {
    #[error("{0}")]
    Context(
        #[source]
        #[from]
        MailContextError,
    ),
    #[error("No user sessions available")]
    NoContext,
    #[error("Label {0}({1})has no remote id")]
    LabelHasNoRemoteId(String, LocalLabelId),
}

pub struct MailboxState {
    user_context: Option<MailboxUserContextState>,
    active_label: Option<LocalLabel>,
    labels_loading_state: LoadingState,
    conversation_loading_state: LoadingState,
}

pub struct MailboxUserContextState {
    context: MailUserContext,
    pub conversations: ConversationsLiveQuery,
    pub system_labels: LabelsByTypeWithConversationCountLiveQuery,
    pub folders: LabelsByTypeWithConversationCountLiveQuery,
    pub labels: LabelsByTypeWithConversationCountLiveQuery,
}

impl MailboxUserContextState {
    pub fn new(context: MailUserContext) -> Self {
        Self {
            conversations: context.new_inbox_conversations_live_query(),
            system_labels: context
                .new_labels_by_type_with_conversation_count_live_query(LabelType::System),
            folders: context
                .new_labels_by_type_with_conversation_count_live_query(LabelType::Folder),
            labels: context.new_labels_by_type_with_conversation_count_live_query(LabelType::Label),
            context,
        }
    }
}

impl MailboxState {
    pub fn new() -> Self {
        Self {
            user_context: None,
            active_label: None,
            labels_loading_state: LoadingState::Unloaded,
            conversation_loading_state: LoadingState::Unloaded,
        }
    }

    pub fn labels_loading_state(&self) -> LoadingState {
        self.labels_loading_state
    }

    pub fn conversation_loading_state(&self) -> LoadingState {
        self.conversation_loading_state
    }

    pub fn active_label(&self) -> Option<&LocalLabel> {
        self.active_label.as_ref()
    }

    pub fn active_label_type(&self) -> LabelType {
        self.active_label
            .as_ref()
            .map(|l| l.label_type)
            .unwrap_or(LabelType::System)
    }

    pub fn active_label_name(&self) -> &str {
        if let Some(label) = &self.active_label {
            label.path.as_deref().unwrap_or(label.name.as_str())
        } else {
            "Inbox"
        }
    }

    pub fn mailbox_context(&self) -> Option<&MailboxUserContextState> {
        self.user_context.as_ref()
    }

    fn on_refresh(
        &mut self,
        mail_context: &MailContext,
        mut app_dispatcher: AppLocalDispatcher<AppState, AppEvent>,
    ) {
        let Some(context) = &self.user_context else {
            app_dispatcher.set_error("Mailbox Error", MailboxStateError::NoContext);
            return;
        };

        self.labels_loading_state = LoadingState::Loading;
        self.conversation_loading_state = LoadingState::Loading;

        let label_id = if let Some(label) = &self.active_label {
            let Some(id) = &label.rid else {
                app_dispatcher.set_error(
                    "Mailbox Error",
                    MailboxStateError::LabelHasNoRemoteId(label.name.clone(), label.id),
                );
                return;
            };
            id.clone()
        } else {
            SysLabelId::INBOX.into()
        };
        context.context.initialize(
            mail_context,
            label_id,
            Box::new(MailboxInitCallback(app_dispatcher.background_dispatcher())),
        );
    }

    fn on_mailbox_open(
        &mut self,
        app_dispatcher: AppLocalDispatcher<AppState, AppEvent>,
        mail_context: &MailContext,
        user_context: MailUserContext,
    ) {
        self.user_context = Some(MailboxUserContextState::new(user_context));
        self.on_refresh(mail_context, app_dispatcher);
    }
    pub fn load_label(
        &mut self,
        mail_context: &MailContext,
        label: LocalLabel,
        mut dispatcher: AppLocalDispatcher<AppState, AppEvent>,
    ) {
        let Some(mailbox_context) = &mut self.user_context else {
            dispatcher.set_error("Mailbox Error", MailboxStateError::NoContext);
            return;
        };
        let Some(remote_label_id) = label.rid.clone() else {
            dispatcher.set_error(
                "Mailbox Error",
                MailboxStateError::LabelHasNoRemoteId(label.name.clone(), label.id),
            );
            return;
        };
        mailbox_context.conversations = mailbox_context
            .context
            .new_conversation_live_query(label.id);
        self.active_label = Some(label);
        self.conversation_loading_state = LoadingState::Loading;
        let context = mailbox_context.context.clone();
        let dispatcher = dispatcher.background_dispatcher();
        mail_context.async_runtime().spawn(async move {
            let result = context
                .sync_first_conversation_page(remote_label_id, 50)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to load conversations: {e}");
                    e.into()
                });
            dispatcher
                .queue_event_async(MailboxEvent::LoadConversations(result))
                .await;
        });
    }

    pub fn handle_event(
        &mut self,
        mut dispatcher: AppLocalDispatcher<AppState, AppEvent>,
        mail_context: &MailContext,
        event: MailboxEvent,
    ) {
        match event {
            MailboxEvent::LoadLabelRequest(label) => {
                self.load_label(mail_context, label, dispatcher);
            }
            MailboxEvent::MailboxRefresh => {
                self.on_refresh(mail_context, dispatcher);
            }
            MailboxEvent::NewMailboxSession(context) => {
                dispatcher.push_view(ConversationView::new());
                self.on_mailbox_open(dispatcher, mail_context, context);
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
                    mail_context.async_runtime().spawn(async move {
                        if let Err(e) = mailbox_context.context.logout().await {
                            bg_dispatcher.set_error("Failed to Logout", e);
                        }
                        bg_dispatcher.queue_on_main(|app| {
                            app.pop_all_views();
                            app.push_view(SessionsView::new());
                        });
                    });
                }
            }
        }
    }
}

pub struct MailboxInitCallback(AppBackgroundDispatcher<AppState, AppEvent>);
impl MailUserContextInitializationCallback for MailboxInitCallback {
    fn on_stage(&self, stage: MailUserContextLoadingStage) {
        match stage {
            MailUserContextLoadingStage::Conversation => {
                self.0.queue_event(MailboxEvent::LoadLabels(Ok(())))
            }
            MailUserContextLoadingStage::Finished => {
                self.0.queue_event(MailboxEvent::LoadConversations(Ok(())))
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
