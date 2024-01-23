mod provider;
mod store;

#[cfg(feature = "uniffi")]
pub mod uniffi_bindgen;

use proton_api_mail::domain::{Label, LabelEvent, LabelId, LabelType};
use proton_api_mail::proton_api_core::domain::EventAction;
use proton_api_mail::proton_api_core::exports::{anyhow, anyhow::anyhow, thiserror};
use proton_api_mail::proton_api_core::http;
pub use provider::*;
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

const fn label_type_to_index(label_type: LabelType) -> usize {
    label_type as usize - 1
}

pub struct Labels {
    provider: Box<dyn Provider>,
    store: Box<dyn Store>,
    labels: [Vec<Label>; LABEL_CATEGORIES.len()],
    cb: Box<dyn Callback>,
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
    pub fn new(provider: Box<dyn Provider>, store: Box<dyn Store>, cb: Box<dyn Callback>) -> Self {
        Self {
            provider,
            store,
            labels: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
            cb,
        }
    }

    pub async fn initialize_from_provider(&mut self) -> LabelsResult<()> {
        let mut writer = self.store.write().await;

        for category in LABEL_CATEGORIES {
            let mut labels = self.provider.get_labels(category).await?;
            writer.store(&labels).await.map_err(LabelsError::Store)?;
            labels.sort_by(|l1, l2| l1.order.cmp(&l2.order));

            self.labels[label_type_to_index(category)] = labels
        }

        Ok(())
    }

    pub fn len(&self) -> usize {
        self.labels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }

    pub fn get_with_type(&self, label_id: &LabelId, label_type: LabelType) -> Option<&Label> {
        self.labels[label_type_to_index(label_type)]
            .iter()
            .find(|&l| l.id == *label_id)
    }

    pub fn get(&self, label_id: &LabelId) -> Option<&Label> {
        for label_type in LABEL_CATEGORIES {
            let Some(l) = self.get_with_type(label_id, label_type) else {
                continue;
            };

            return Some(l);
        }

        None
    }

    pub fn get_with_type_mut(
        &mut self,
        label_id: &LabelId,
        label_type: LabelType,
    ) -> Option<&mut Label> {
        self.labels[label_type_to_index(label_type)]
            .iter_mut()
            .find(|l| l.id == *label_id)
    }

    pub fn get_labels_by_type(&self, label_type: LabelType) -> &[Label] {
        &self.labels[label_type_to_index(label_type)]
    }

    pub async fn create_label(
        &mut self,
        name: &str,
        color: &str,
        label_type: LabelType,
        parent_id: Option<&LabelId>,
    ) -> LabelsResult<Label> {
        let label = self
            .provider
            .create_label(name, color, label_type, parent_id)
            .await?;
        let mut writer = self.store.write().await;
        writer.store_one(&label).await.map_err(LabelsError::Store)?;

        self.labels[label_type_to_index(label_type)].push(label.clone());
        self.cb.label_created(&label);
        Ok(label)
    }

    fn find_label_index(&self, label_id: &LabelId) -> Option<(LabelType, usize)> {
        for category in LABEL_CATEGORIES {
            if let Some((index, _)) = self.labels[label_type_to_index(category)]
                .iter()
                .enumerate()
                .find(|(_, l)| l.id == *label_id)
            {
                return Some((category, index));
            }
        }

        None
    }

    pub async fn update_label(
        &mut self,
        label_id: &LabelId,
        name: &str,
        color: &str,
        parent_id: Option<&LabelId>,
    ) -> LabelsResult<Label> {
        let (label, order_changed) = {
            let Some((label_type, index)) = self.find_label_index(label_id) else {
                return Err(LabelsError::NotExist(label_id.clone()));
            };

            let updated_label = self
                .provider
                .update_label(label_id, name, color, parent_id)
                .await?;

            let mut writer = self.store.write().await;
            writer
                .update(&updated_label)
                .await
                .map_err(LabelsError::Store)?;

            let label = &mut self.labels[label_type_to_index(label_type)][index];

            let order_changed = updated_label.order != label.order;
            *label = updated_label;
            self.cb.label_updated(label);

            (label.clone(), order_changed)
        };

        if order_changed {
            self.rebuild_sorted_label_vec(label.label_type);
        }
        Ok(label)
    }

    pub async fn delete_label(&mut self, label_id: &LabelId) -> LabelsResult<()> {
        self.provider.delete_label(label_id).await?;
        let mut writer = self.store.write().await;
        writer.delete(label_id).await.map_err(LabelsError::Store)?;
        for l in &mut self.labels {
            l.retain(|l| l.id != *label_id)
        }
        self.cb.label_deleted(label_id);
        Ok(())
    }

    fn rebuild_sorted_label_vec(&mut self, label_type: LabelType) {
        let labels = &mut self.labels[label_type_to_index(label_type)];

        labels.sort_by(|l1, l2| l1.order.cmp(&l2.order));
        //TODO: Optimize update callback
        for l in labels {
            self.cb.label_updated(l);
        }
    }

    pub async fn on_events(&mut self, events: &[LabelEvent]) -> LabelsResult<()> {
        //TODO: transactional rollback?
        //TODO: Order updates
        //TODO: Fix deadlock if callback calls labels, callbacks should be deferred until all label
        // operations are applied.
        let mut writer = self.store.write().await;

        for event in events {
            match event.action {
                EventAction::Delete => {
                    writer.delete(&event.id).await.map_err(LabelsError::Store)?;
                    for l in &mut self.labels {
                        l.retain(|l| l.id != event.id)
                    }
                    self.cb.label_deleted(&event.id);
                }
                EventAction::Create => {
                    let label = event.label.as_ref().ok_or(LabelsError::Unknown(anyhow!(
                        "Label data missing from create label event"
                    )))?;

                    writer.store_one(label).await.map_err(LabelsError::Store)?;
                    if let Some(existing_label) = self.labels[label_type_to_index(label.label_type)]
                        .iter_mut()
                        .find(|el| el.id == label.id)
                    {
                        *existing_label = label.clone();
                        self.cb.label_updated(label);
                        continue;
                    }

                    self.labels[label_type_to_index(label.label_type)].push(label.clone());
                    self.cb.label_created(label);
                }
                EventAction::Update | EventAction::UpdateFlags => {
                    let label = event.label.as_ref().ok_or(LabelsError::Unknown(anyhow!(
                        "Label data missing from update label event"
                    )))?;
                    writer.update(label).await.map_err(LabelsError::Store)?;
                    if let Some(existing_label) = self.labels[label_type_to_index(label.label_type)]
                        .iter_mut()
                        .find(|l| l.id == label.id)
                    {
                        *existing_label = label.clone();
                        self.cb.label_updated(label);
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();
