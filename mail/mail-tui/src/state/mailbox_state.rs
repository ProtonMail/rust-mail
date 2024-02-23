use crate::app::AppDispatcher;
use crate::events::AppEvents;
use crate::state::{AppState, DataLoadError, UserState};
use anyhow::anyhow;
use proton_api_mail::domain::{
    ConversationMetadataFilterBuilder, LabelId, LabelType, SysLabelId, ALL_LABEL_TYPES,
};
use proton_api_mail::proton_api_core::exports::tracing;
use proton_api_mail::MailSession;
use proton_async::runtime::MTRuntime;
use proton_mail_db::{
    LabelColor, LocalConversationWithContext, LocalLabel, LocalLabelId, MailSqliteConnectionPool,
};

pub struct MailboxState {
    active_label: LocalLabel,
    label_list: [Vec<LocalLabel>; ALL_LABEL_TYPES.len()],
    pub conversation_list: Vec<LocalConversationWithContext>,
}

const fn label_type_to_index(l: LabelType) -> usize {
    (l as usize) - 1
}
impl MailboxState {
    pub fn new() -> Self {
        Self {
            active_label: LocalLabel {
                id: LocalLabelId::new(u64::MAX),
                rid: Some(SysLabelId::INBOX.into()),
                parent_id: None,
                name: "".to_string(),
                path: None,
                color: LabelColor::black(),
                label_type: LabelType::System,
                order: 0,
                notified: false,
                expanded: false,
                sticky: false,
            },
            label_list: Default::default(),
            conversation_list: Default::default(),
        }
    }

    pub fn active_label(&self) -> &LocalLabel {
        &self.active_label
    }

    pub fn active_label_name(&self) -> &str {
        self.active_label
            .path
            .as_deref()
            .unwrap_or(self.active_label.name.as_str())
    }

    pub fn assign_labels(&mut self, all_labels: Vec<LocalLabel>) {
        for label in all_labels {
            self.label_list[label_type_to_index(label.label_type)].push(label);
        }

        for l in &mut self.label_list {
            l.sort_by(|l1, l2| l1.order.cmp(&l2.order))
        }
    }

    pub fn label_list(&self, label_type: LabelType) -> &[LocalLabel] {
        &self.label_list[label_type_to_index(label_type)]
    }

    pub fn reset(&mut self) {
        for l in &mut self.label_list {
            l.clear();
        }
        self.conversation_list.clear();
    }

    pub fn first_load(
        &self,
        user_state: &UserState,
        app_dispatcher: AppDispatcher<AppState, AppEvents>,
        runtime: &MTRuntime,
    ) {
        let remote_label_id = SysLabelId::INBOX.into();
        let session = user_state.session.clone();
        let db = user_state.db_pool.clone();
        runtime.spawn(async move {
            let labels = match load_labels(&session, &db).await {
                Ok(l) => l,
                Err(e) => {
                    app_dispatcher
                        .queue_event_async(AppEvents::mailbox_label_load(Err(e)))
                        .await;
                    return;
                }
            };

            // resolve local label id
            let Some(label_id) = labels
                .iter()
                .find(|l| l.rid.as_ref() == Some(&remote_label_id))
                .map(|l| l.id)
            else {
                app_dispatcher
                    .queue_event_async(AppEvents::mailbox_label_load(Err(DataLoadError::Other(
                        anyhow!("Failed to find local label if for {remote_label_id}"),
                    ))))
                    .await;
                return;
            };
            app_dispatcher.queue_event(AppEvents::mailbox_label_load(Ok(labels)));

            app_dispatcher
                .queue_event_async(AppEvents::mailbox_conversation_load(
                    load_conversations(&session, &db, label_id, &remote_label_id).await,
                ))
                .await;
        });
    }

    pub fn load_label(
        &mut self,
        label: LocalLabel,
        user_state: &UserState,
        app_dispatcher: AppDispatcher<AppState, AppEvents>,
        runtime: &MTRuntime,
    ) {
        self.active_label = label;
        let session = user_state.session.clone();
        let db = user_state.db_pool.clone();
        let label_id = self.active_label.id;
        let Some(remote_label_id) = self.active_label.rid.clone() else {
            app_dispatcher.set_error(
                "Invalid State",
                DataLoadError::Other(anyhow!(
                    "Local label {}({}) has no remote id",
                    self.active_label.name,
                    self.active_label.id
                )),
            );
            return;
        };
        runtime.spawn(async move {
            let conv = load_conversations(&session, &db, label_id, &remote_label_id)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to load conversations: {e}");
                    e
                });
            app_dispatcher
                .queue_event_async(AppEvents::mailbox_conversation_load(conv))
                .await;
        });
    }
}

async fn load_labels(
    session: &MailSession,
    pool: &MailSqliteConnectionPool,
) -> Result<Vec<LocalLabel>, DataLoadError> {
    {
        let db = pool.acquire()?;
        let labels = db.as_connection_ref().get_all_local_labels()?;
        if !labels.is_empty() {
            return Ok(labels);
        }
    }
    let mut all_labels = Vec::new();
    for category in ALL_LABEL_TYPES {
        let labels = session.get_labels(category).await?;
        all_labels.extend(labels);
    }

    let mut db = pool.acquire()?;
    let mut tx = db.tx()?;
    let mut conn = tx.as_connection_mut();
    conn.create_remote_labels(all_labels.iter())?;
    tx.commit()?;

    Ok(db.as_connection_ref().get_all_local_labels()?)
}

#[tracing::instrument(skip(session,pool),fields(label_id=?label_id))]
async fn load_conversations(
    session: &MailSession,
    pool: &MailSqliteConnectionPool,
    label_id: LocalLabelId,
    remote_label_id: &LabelId,
) -> Result<Vec<LocalConversationWithContext>, DataLoadError> {
    tracing::debug!("Loading conversations");
    let filter = ConversationMetadataFilterBuilder::new(0, 25)
        .descending()
        .with_label_id(remote_label_id.clone())
        .build();
    let remote_conversations = session.get_conversations(filter).await?;

    tracing::debug!(
        "Storing {} conversations in db",
        remote_conversations.conversations.len()
    );
    let mut db = pool.acquire()?;
    let mut tx = db.tx()?;
    let mut conn = tx.as_connection_mut();
    conn.create_conversations(remote_conversations.conversations.iter())?;
    tx.commit()?;

    let conversations = db
        .as_connection_ref()
        .get_conversations_with_context(label_id, 25)?;
    tracing::debug!("Retrieved {} conversation from db", conversations.len());
    Ok(conversations)
}
