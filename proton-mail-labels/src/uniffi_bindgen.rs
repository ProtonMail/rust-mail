use crate::{LabelsResult, MemoryStore, ProtonProvider};
use proton_api_mail::domain::{Label, LabelId, LabelType, MailEvent};
use proton_api_mail::proton_api_core::exports::anyhow;
use proton_api_mail::{proton_api_core, MailSession};
use proton_async::{async_trait, tokio};
use proton_event_loop::{LoopError, LoopErrorHandler, LoopErrorHandlerReply, SubscriberError};
use std::sync::Arc;
use std::time::Duration;

#[derive(uniffi::Object, Clone)]
pub struct EventLoop(pub proton_event_loop::Loop<MailEvent>);

proton_event_loop::gen_event_loop_error_handler!(MailEventLoopErrorHandler, UniffiLoopErrorHandler);

#[uniffi::export]
impl EventLoop {
    pub fn resume(&self) {
        self.0.resume()
    }

    pub fn pause(&self) {
        self.0.pause()
    }
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn new_event_loop(
    session: &proton_api_core::uniffi_bindgen::Session,
    error_handler: Box<dyn MailEventLoopErrorHandler>,
) -> Result<Arc<EventLoop>, LoopError> {
    let event_provider = proton_event_loop::ProtonProvider::new(session.0.clone());
    let event_store = proton_event_loop::InMemoryStore::default();
    let event_error_handler = UniffiLoopErrorHandler(error_handler);
    let eloop = proton_event_loop::Loop::new();

    //TODO: task handle?
    eloop
        .start(
            Duration::from_secs(10),
            Box::new(event_store),
            Box::new(event_provider),
            Box::new(event_error_handler),
        )
        .await?;

    Ok(Arc::new(EventLoop(eloop)))
}
#[uniffi::export(callback_interface)]
pub trait Callback: Send + Sync {
    fn label_created(&self, label: Label);
    fn label_updated(&self, label: Label);

    // There is not custom new type for LabelId in the biding glue code.
    fn label_deleted(&self, id: String);
}

struct UniffiCallback(Box<dyn Callback>);

impl crate::Callback for UniffiCallback {
    fn label_created(&self, label: &Label) {
        self.0.label_created(label.clone());
    }

    fn label_updated(&self, label: &Label) {
        self.0.label_updated(label.clone());
    }

    fn label_deleted(&self, id: &LabelId) {
        self.0.label_deleted(id.clone().0);
    }
}

#[derive(uniffi::Object)]
pub struct Labels(tokio::sync::RwLock<crate::Labels>);
#[uniffi::export(async_runtime = "tokio")]
impl Labels {
    #[uniffi::constructor]
    pub fn new(
        session: &proton_api_core::uniffi_bindgen::Session,
        callback: Box<dyn Callback>,
    ) -> Arc<Self> {
        let label_provider = ProtonProvider::new(MailSession::new(session.0.clone()));
        let label_store = MemoryStore::new();

        Arc::new(Labels(tokio::sync::RwLock::new(crate::Labels::new(
            Box::new(label_provider),
            Box::new(label_store),
            Box::new(UniffiCallback(callback)),
        ))))
    }

    pub async fn initialize_from_provider(&self) -> LabelsResult<()> {
        let mut accessor = self.0.write().await;
        accessor.initialize_from_provider().await
    }

    pub async fn create_label(
        &self,
        name: String,
        color: String,
        label_type: LabelType,
        parent_id: Option<String>,
    ) -> LabelsResult<Label> {
        let parent_id = parent_id.map(LabelId::from);
        let mut accessor = self.0.write().await;
        accessor
            .create_label(&name, &color, label_type, parent_id.as_ref())
            .await
    }

    pub async fn update_label(
        &self,
        label_id: String,
        name: String,
        color: String,
        parent_id: Option<String>,
    ) -> LabelsResult<Label> {
        let label_id = LabelId::from(label_id);
        let parent_id = parent_id.map(LabelId::from);
        let mut accessor = self.0.write().await;
        accessor
            .update_label(&label_id, &name, &color, parent_id.as_ref())
            .await
    }

    pub async fn delete_label(&self, label_id: String) -> LabelsResult<()> {
        let label_id = LabelId::from(label_id);
        let mut accessor = self.0.write().await;
        accessor.delete_label(&label_id).await
    }

    pub async fn get_label(&self, label_id: String) -> Option<Label> {
        let accessor = self.0.read().await;
        let label_id = LabelId::from(label_id);
        accessor.get(&label_id).cloned()
    }

    pub async fn get_ordered_labels(&self) -> Vec<Label> {
        let accessor = self.0.read().await;
        accessor.get_ordered_labels()
    }
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn labels_subscribe_event_loop(labels: Arc<Labels>, event_loop: &EventLoop) {
    event_loop
        .0
        .subscribe(Box::new(LabelSubscriber(labels)))
        .await
}

struct LabelSubscriber(Arc<Labels>);

#[async_trait::async_trait]
impl proton_event_loop::Subscriber<MailEvent> for LabelSubscriber {
    fn name(&self) -> &str {
        "LabelTestSubscriber"
    }

    async fn on_events(&mut self, events: &[MailEvent]) -> Result<(), SubscriberError> {
        let mut accessor = self.0 .0.write().await;
        for evt in events {
            if let Some(events) = &evt.labels {
                if let Err(e) = accessor.on_events(events).await {
                    return Err(SubscriberError::Other(anyhow::anyhow!(
                        "Failed to apply event ({}): {e}",
                        evt.event_id
                    )));
                }
            }
        }

        Ok(())
    }
}
