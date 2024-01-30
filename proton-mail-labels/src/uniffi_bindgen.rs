use crate::{LabelsResult, MemoryStore, ProtonProvider};
use proton_api_core::exports::{parking_lot, thiserror};
use proton_api_core::http::HttpRequestError;
use proton_api_mail::domain::{Label, LabelId, LabelType, MailEvent};
use proton_api_mail::proton_api_core::exports::anyhow;
use proton_api_mail::{proton_api_core, MailSession};
use proton_async::async_trait;
use std::sync::Arc;
use std::time::Duration;

proton_event_loop::gen_event_loop_uniffi_types!(Mail, MailEvent);

#[uniffi::export]
pub fn new_event_loop(
    session: &proton_api_core::uniffi_bindgen::Session,
    error_handler: Box<dyn MailLoopErrorHandler>,
) -> Result<Arc<MailEventLoop>, MailLoopError> {
    crate::RUNTIME.block_on(async {
        let eloop = MailEventLoop::new();

        eloop.start_poller(session, error_handler).await?;

        Ok(eloop)
    })
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

type SharedLabels = Arc<parking_lot::RwLock<crate::Labels>>;
#[derive(uniffi::Object)]
pub struct Labels(SharedLabels);
#[uniffi::export]
impl Labels {
    #[uniffi::constructor]
    pub fn new(
        session: &proton_api_core::uniffi_bindgen::Session,
        callback: Box<dyn Callback>,
    ) -> Arc<Self> {
        let label_provider = ProtonProvider::new(MailSession::new(session.0.clone()));
        let label_store = MemoryStore::new();

        Arc::new(Labels(Arc::new(parking_lot::RwLock::new(
            crate::Labels::new(
                Box::new(label_provider),
                Box::new(label_store),
                Box::new(UniffiCallback(callback)),
            ),
        ))))
    }

    pub fn initialize_from_provider(&self) -> LabelsResult<()> {
        let mut accessor = self.0.write();
        accessor.initialize_from_provider()
    }

    pub fn count(&self) -> u64 {
        let accessor = self.0.read();
        accessor.len() as u64
    }

    pub fn create_label(
        &self,
        name: String,
        color: String,
        label_type: LabelType,
        parent_id: Option<String>,
    ) -> LabelsResult<Label> {
        let parent_id = parent_id.map(LabelId::from);
        let mut accessor = self.0.write();
        accessor.create_label(&name, &color, label_type, parent_id.as_ref())
    }

    pub fn update_label(
        &self,
        label_id: String,
        name: String,
        color: String,
        parent_id: Option<String>,
    ) -> LabelsResult<Label> {
        let label_id = LabelId::from(label_id);
        let parent_id = parent_id.map(LabelId::from);
        let mut accessor = self.0.write();
        accessor.update_label(&label_id, &name, &color, parent_id.as_ref())
    }

    pub fn delete_label(&self, label_id: String) -> LabelsResult<()> {
        let label_id = LabelId::from(label_id);
        let mut accessor = self.0.write();
        accessor.delete_label(&label_id)
    }

    pub fn get_label(&self, label_id: String) -> Option<Label> {
        let accessor = self.0.read();
        let label_id = LabelId::from(label_id);
        accessor.get(&label_id).cloned()
    }

    pub fn get_labels(&self, label_type: LabelType) -> Vec<Label> {
        let accessor = self.0.read();
        accessor.get_labels_by_type(label_type).to_vec()
    }

    pub fn len(&self) -> u64 {
        let accessor = self.0.read();
        accessor.len() as u64
    }
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn labels_subscribe_event_loop(labels: Arc<Labels>, event_loop: &MailEventLoop) {
    event_loop
        .0
        .subscribe(Box::new(LabelSubscriber(labels.0.clone())))
        .await
}

struct LabelSubscriber(SharedLabels);

#[async_trait::async_trait]
impl proton_event_loop::Subscriber<MailEvent> for LabelSubscriber {
    fn name(&self) -> &str {
        "LabelTestSubscriber"
    }

    async fn on_events(
        &mut self,
        events: &[MailEvent],
    ) -> Result<(), proton_event_loop::SubscriberError> {
        let mut accessor = self.0.write();
        for evt in events {
            if let Some(events) = &evt.labels {
                if let Err(e) = accessor.on_events(events) {
                    return Err(proton_event_loop::SubscriberError::Other(anyhow::anyhow!(
                        "Failed to apply event ({}): {e}",
                        evt.event_id
                    )));
                }
            }
        }

        Ok(())
    }
}

#[derive(uniffi::Object)]
pub struct LabelView(SharedLabels, LabelType);
#[uniffi::export]
impl LabelView {
    #[uniffi::constructor]
    pub fn new(labels: &Labels, label_type: LabelType) -> Arc<Self> {
        Arc::new(Self(labels.0.clone(), label_type))
    }

    pub fn len(&self) -> i64 {
        let accessor = self.0.read();
        accessor.get_labels_by_type(self.1).len() as i64
    }

    pub fn at(&self, index: i64) -> Option<Label> {
        debug_assert!(index >= 0);
        let accessor = self.0.read();
        accessor
            .get_labels_by_type(self.1)
            .get(index as usize)
            .cloned()
    }
}
