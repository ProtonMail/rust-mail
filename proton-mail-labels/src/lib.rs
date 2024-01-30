mod provider;
mod store;

#[cfg(feature = "uniffi")]
pub mod uniffi_bindgen;

use lazy_static::lazy_static;
use proton_api_mail::domain::{Label, LabelEvent, LabelId, LabelType};
use proton_api_mail::proton_api_core::domain::EventAction;
use proton_api_mail::proton_api_core::exports::{anyhow, anyhow::anyhow, parking_lot, thiserror};
use proton_api_mail::proton_api_core::http;
pub use provider::*;
use std::ops::Deref;
use std::sync::Arc;
pub use store::*;

pub trait Callback: Send + Sync {
    fn label_created(&self, label: &Label);
    fn label_updated(&self, label: &Label);

    fn label_deleted(&self, id: &LabelId);
}

const LABEL_CATEGORIES: [LabelType; 4] = [
    LabelType::System,
    LabelType::Label,
    LabelType::Folder,
    LabelType::ContactGroup,
];

pub struct Labels {
    provider: Box<dyn Provider>,
    store: Box<dyn Store>,
    cb: Vec<Box<dyn Callback>>,
}

#[derive(Debug, thiserror::Error)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Error))]
#[cfg_attr(feature = "uniffi", uniffi(flat_error))]
pub enum LabelsError {
    #[error("{0}")]
    Provider(#[from] http::HttpRequestError),
    #[error("{0}")]
    Store(#[source] anyhow::Error),
    #[error("Label {0} does not exists")]
    NotExist(LabelId),
    #[error("Unknown error:{0}")]
    Unknown(anyhow::Error),
}

pub type LabelsResult<T> = Result<T, LabelsError>;

impl Labels {
    pub fn new(provider: Box<dyn Provider>, store: Box<dyn Store>) -> Self {
        Self {
            provider,
            store,
            cb: Vec::new(),
        }
    }

    pub fn initialize_from_provider(&mut self) -> LabelsResult<()> {
        let mut writer = self.store.write();
        for category in LABEL_CATEGORIES {
            let labels = RUNTIME.block_on(async { self.provider.get_labels(category).await })?;
            writer.store(&labels).map_err(LabelsError::Store)?;
        }

        Ok(())
    }

    pub fn len(&self) -> usize {
        self.store.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn add_callback(&mut self, cb: Box<dyn Callback>) {
        self.cb.push(cb);
    }

    pub fn get_labels_by_type(&self, label_type: LabelType) -> LabelsResult<Vec<Label>> {
        self.store
            .read()
            .get_all_with_type(label_type)
            .map_err(LabelsError::Store)
    }

    pub fn create_label(
        &mut self,
        name: &str,
        color: &str,
        label_type: LabelType,
        parent_id: Option<&LabelId>,
    ) -> LabelsResult<Label> {
        let label = RUNTIME.block_on(async {
            self.provider
                .create_label(name, color, label_type, parent_id)
                .await
        })?;
        let mut writer = self.store.write();
        writer.store_one(&label).map_err(LabelsError::Store)?;

        self.for_each_cb(|cb| {
            cb.label_created(&label);
        });
        Ok(label)
    }

    pub fn update_label(
        &mut self,
        label_id: &LabelId,
        name: &str,
        color: &str,
        parent_id: Option<&LabelId>,
    ) -> LabelsResult<Label> {
        let updated_label = RUNTIME.block_on(async {
            self.provider
                .update_label(label_id, name, color, parent_id)
                .await
        })?;

        {
            let mut writer = self.store.write();
            writer.update(&updated_label).map_err(LabelsError::Store)?;
        }

        self.for_each_cb(|cb| {
            cb.label_updated(&updated_label);
        });

        Ok(updated_label)
    }

    pub fn delete_label(&mut self, label_id: &LabelId) -> LabelsResult<()> {
        RUNTIME.block_on(async { self.provider.delete_label(label_id).await })?;
        let mut writer = self.store.write();
        writer.delete(label_id).map_err(LabelsError::Store)?;
        self.for_each_cb(|cb| {
            cb.label_deleted(label_id);
        });

        Ok(())
    }

    fn for_each_cb(&self, f: impl Fn(&dyn Callback)) {
        for cb in &self.cb {
            (f)(cb.deref());
        }
    }

    pub fn on_events(&mut self, events: &[LabelEvent]) -> LabelsResult<()> {
        let mut pending_ops = Vec::new();

        //TODO: transactional rollback?
        //TODO: Order updates
        //TODO: Fix deadlock if callback calls labels, callbacks should be deferred until all label
        // operations are applied.
        let mut writer = self.store.write();

        for event in events {
            match event.action {
                EventAction::Delete => {
                    writer.delete(&event.id).map_err(LabelsError::Store)?;
                    pending_ops.push(PendingLabelOp::Delete(event.id.clone()));
                }
                EventAction::Create => {
                    let label = event.label.as_ref().ok_or(LabelsError::Unknown(anyhow!(
                        "Label data missing from create label event"
                    )))?;

                    writer.store_one(label).map_err(LabelsError::Store)?;
                    pending_ops.push(PendingLabelOp::Create(label.clone()));
                }
                EventAction::Update | EventAction::UpdateFlags => {
                    let label = event.label.as_ref().ok_or(LabelsError::Unknown(anyhow!(
                        "Label data missing from update label event"
                    )))?;
                    writer.update(label).map_err(LabelsError::Store)?;
                    pending_ops.push(PendingLabelOp::Update(label.clone()));
                }
            }
        }

        for op in pending_ops {
            match op {
                PendingLabelOp::Create(label) => {
                    self.for_each_cb(|cb| {
                        cb.label_created(&label);
                    });
                }
                PendingLabelOp::Update(label) => {
                    self.for_each_cb(|cb| {
                        cb.label_updated(&label);
                    });
                }
                PendingLabelOp::Delete(id) => {
                    self.for_each_cb(|cb| {
                        cb.label_deleted(&id);
                    });
                }
            }
        }

        Ok(())
    }
}

enum PendingLabelOp {
    Create(Label),
    Update(Label),
    Delete(LabelId),
}

pub struct LabelView {
    labels: Vec<Label>,
    label_type: LabelType,
    pending: LabelViewCallback,
}

impl LabelView {
    pub fn new(
        labels: &mut Labels,
        label_type: LabelType,
        ui_cb: Box<dyn UILabelViewCallback>,
    ) -> LabelsResult<Self> {
        let l = labels.get_labels_by_type(label_type)?;
        let mut view = Self {
            labels: l,
            label_type,
            pending: LabelViewCallback::new(label_type, ui_cb),
        };
        view.sort();

        labels.add_callback(Box::new(view.pending.clone()));

        Ok(view)
    }

    pub fn label_type(&self) -> LabelType {
        self.label_type
    }

    pub fn len(&self) -> usize {
        self.labels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&Label> {
        self.labels.get(index)
    }

    pub fn has_pending_changes(&self) -> bool {
        !self.pending.inner.lock().pending.is_empty()
    }

    pub fn consume_pending_changes(&mut self) -> bool {
        let pending = { self.pending.inner.lock().get_pending() };
        let has_changes = !pending.is_empty();
        self.apply_changed(pending);

        has_changes
    }

    fn sort(&mut self) {
        self.labels.sort_by(|l1, l2| l1.order.cmp(&l2.order));
    }

    fn apply_changed(&mut self, pending: Vec<PendingLabelOp>) {
        let mut resort = false;
        for op in pending {
            match op {
                PendingLabelOp::Create(label) => {
                    if let Some(l) = self.labels.iter_mut().find(|l| l.id == label.id) {
                        resort = resort || l.order != label.order;
                        *l = label;
                    } else {
                        self.labels.push(label);
                    }
                }
                PendingLabelOp::Update(label) => {
                    if let Some(l) = self.labels.iter_mut().find(|l| l.id == label.id) {
                        resort = resort || l.order != label.order;
                        *l = label;
                    }
                }
                PendingLabelOp::Delete(id) => self.labels.retain(|x| x.id != id),
            }
        }

        if resort {
            self.sort();
        }
    }
}

impl AsRef<[Label]> for LabelView {
    fn as_ref(&self) -> &[Label] {
        &self.labels
    }
}

pub trait UILabelViewCallback: Send + Sync {
    fn on_pending(&self);
}

#[derive(Clone)]
struct LabelViewCallback {
    inner: Arc<parking_lot::Mutex<LabelViewCallbackInner>>,
}

impl LabelViewCallback {
    fn new(label_type: LabelType, ui_callback: Box<dyn UILabelViewCallback>) -> Self {
        Self {
            inner: Arc::new(parking_lot::Mutex::new(LabelViewCallbackInner::new(
                label_type,
                ui_callback,
            ))),
        }
    }
}

struct LabelViewCallbackInner {
    pending: Vec<PendingLabelOp>,
    label_type: LabelType,
    ui_callback: Box<dyn UILabelViewCallback>,
}

impl LabelViewCallbackInner {
    pub fn new(label_type: LabelType, ui_callback: Box<dyn UILabelViewCallback>) -> Self {
        Self {
            pending: Vec::new(),
            label_type,
            ui_callback,
        }
    }
}

impl LabelViewCallbackInner {
    fn label_add(&mut self, label: Label) {
        if label.label_type == self.label_type {
            self.pending.push(PendingLabelOp::Create(label));
            self.ui_callback.on_pending();
        }
    }

    pub fn label_updated(&mut self, label: Label) {
        if label.label_type == self.label_type {
            self.pending.push(PendingLabelOp::Create(label));
            self.ui_callback.on_pending();
        }
    }

    fn label_deleted(&mut self, label_id: LabelId) {
        self.pending.push(PendingLabelOp::Delete(label_id));
        self.ui_callback.on_pending();
    }

    fn get_pending(&mut self) -> Vec<PendingLabelOp> {
        std::mem::take(&mut self.pending)
    }
}

impl Callback for LabelViewCallback {
    fn label_created(&self, label: &Label) {
        self.inner.lock().label_add(label.clone());
    }

    fn label_updated(&self, label: &Label) {
        self.inner.lock().label_updated(label.clone());
    }

    fn label_deleted(&self, id: &LabelId) {
        self.inner.lock().label_deleted(id.clone());
    }
}

lazy_static! {
    static ref RUNTIME: proton_async::tokio::runtime::Runtime = {
        proton_async::tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build runtime")
    };
}

pub fn static_runtime() -> &'static proton_async::tokio::runtime::Runtime {
    &RUNTIME
}

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();
