use crate::{LabelsResult, MemoryStore, ProtonProvider};
use proton_api_core::exports::{parking_lot, thiserror};
use proton_api_core::http::HttpRequestError;
use proton_api_mail::domain::{Label, LabelId, LabelType, MailEvent};
use proton_api_mail::proton_api_core::exports::anyhow;
use proton_api_mail::proton_api_core::exports::parking_lot::lock_api::RwLock;
use proton_api_mail::{proton_api_core, MailSession};
use proton_async::async_trait;
use std::ops::DerefMut;
use std::sync::atomic::{AtomicBool, Ordering};
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
pub trait LabelsCallback: Send + Sync {
    fn label_created(&self, label: Label);
    fn label_updated(&self, label: Label);

    // There is not custom new type for LabelId in the biding glue code.
    fn label_deleted(&self, id: String);
}

struct UniffiCallback(Box<dyn LabelsCallback>);

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
        callback: Box<dyn LabelsCallback>,
    ) -> Arc<Self> {
        let label_provider = ProtonProvider::new(MailSession::new(session.0.clone()));
        let label_store = MemoryStore::new();

        let mut labels = crate::Labels::new(Box::new(label_provider), Box::new(label_store));
        labels.add_callback(Box::new(UniffiCallback(callback)));
        Arc::new(Labels(Arc::new(parking_lot::RwLock::new(labels))))
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

#[uniffi::export(callback_interface)]
pub trait LabelViewCallback: Send + Sync {
    fn on_has_pending(&self);
}
#[derive(uniffi::Object)]
pub struct LabelView {
    view: parking_lot::RwLock<crate::LabelView>,
    cb: Box<dyn LabelViewCallback>,
    cb_performed: AtomicBool,
}

#[uniffi::export]
impl LabelView {
    #[uniffi::constructor]
    pub fn new(
        labels: &Labels,
        label_type: LabelType,
        cb: Box<dyn LabelViewCallback>,
    ) -> LabelsResult<Self> {
        let mut accessor = labels.0.write();
        Ok(Self {
            view: RwLock::new(crate::LabelView::new(accessor.deref_mut(), label_type)?),
            cb,
            cb_performed: AtomicBool::new(false),
        })
    }

    pub fn len(&self) -> i64 {
        let (len, has_pending) = {
            let accessor = self.view.read();
            (accessor.len() as i64, accessor.has_pending_changes())
        };

        // Perform calculation about pending changes here.
        if has_pending {
            if !self.cb_performed.load(Ordering::Acquire) {
                self.cb.on_has_pending();
                self.cb_performed.store(true, Ordering::Release)
            }
        }

        len
    }

    pub fn at(&self, index: i64) -> Option<Label> {
        debug_assert!(index >= 0);
        let accessor = self.view.read();
        accessor.get(index as usize).cloned()
    }

    pub fn consume_pending_changes(&self) {
        self.view.write().consume_pending_changes();
        self.cb_performed.store(false, Ordering::Release);
    }
}
