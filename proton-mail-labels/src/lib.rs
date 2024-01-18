mod provider;
mod store;

#[cfg(feature = "uniffi")]
pub mod uniffi_bindgen;

use proton_api_rs::domain::{EventAction, Label, LabelEvent, LabelId, LabelType};
use proton_api_rs::exports::{anyhow, anyhow::anyhow, thiserror};
use proton_api_rs::http;
pub use provider::*;
use std::cmp::Ordering;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
pub use store::*;

pub trait Callback: Send + Sync {
    fn label_created(&self, label: &Label);
    fn label_updated(&self, label: &Label);

    fn label_deleted(&self, id: &LabelId);
}

pub struct Labels {
    provider: Box<dyn Provider>,
    store: Box<dyn Store>,
    labels: HashMap<LabelId, Label>,
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
            labels: HashMap::with_capacity(8),
            cb,
        }
    }

    pub async fn initialize_from_provider(&mut self) -> LabelsResult<()> {
        let mut writer = self.store.write().await;

        let categories = [
            LabelType::System,
            LabelType::Label,
            LabelType::Folder,
            LabelType::ContactGroup,
        ];

        for category in categories {
            let labels = self.provider.get_labels(category).await?;
            writer.store(&labels).await.map_err(LabelsError::Store)?;
            self.labels
                .extend(labels.into_iter().map(|l| (l.id.clone(), l)));
        }

        Ok(())
    }

    pub fn len(&self) -> usize {
        self.labels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }

    pub fn get(&self, label_id: &LabelId) -> Option<&Label> {
        self.labels.get(label_id)
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
        self.labels.insert(label.id.clone(), label.clone());
        self.cb.label_created(&label);
        Ok(label)
    }

    pub async fn update_label(
        &mut self,
        label_id: &LabelId,
        name: &str,
        color: &str,
        parent_id: Option<&LabelId>,
    ) -> LabelsResult<Label> {
        let Entry::Occupied(mut o) = self.labels.entry(label_id.clone()) else {
            return Err(LabelsError::NotExist(label_id.clone()));
        };

        let label = self
            .provider
            .update_label(label_id, name, color, parent_id)
            .await?;
        let mut writer = self.store.write().await;
        writer.update(&label).await.map_err(LabelsError::Store)?;
        o.insert(label.clone());
        self.cb.label_updated(&label);
        Ok(label)
    }

    pub async fn delete_label(&mut self, label_id: &LabelId) -> LabelsResult<()> {
        self.provider.delete_label(label_id).await?;
        let mut writer = self.store.write().await;
        writer.delete(label_id).await.map_err(LabelsError::Store)?;
        self.labels.remove(label_id);
        self.cb.label_deleted(label_id);
        Ok(())
    }

    pub async fn on_events(&mut self, events: &[LabelEvent]) -> LabelsResult<()> {
        //TODO: transactional rollback?
        let mut writer = self.store.write().await;

        for event in events {
            match event.action {
                EventAction::Delete => {
                    writer.delete(&event.id).await.map_err(LabelsError::Store)?;
                    self.labels.remove(&event.id);
                    self.cb.label_deleted(&event.id);
                }
                EventAction::Create => {
                    let label = event.label.as_ref().ok_or(LabelsError::Unknown(anyhow!(
                        "Label data missing from create label event"
                    )))?;
                    writer.store_one(label).await.map_err(LabelsError::Store)?;
                    self.labels.insert(label.id.clone(), label.clone());
                    self.cb.label_created(label);
                }
                EventAction::Update | EventAction::UpdateFlags => {
                    let label = event.label.as_ref().ok_or(LabelsError::Unknown(anyhow!(
                        "Label data missing from update label event"
                    )))?;
                    writer.update(label).await.map_err(LabelsError::Store)?;
                    self.labels.insert(label.id.clone(), label.clone());
                    self.cb.label_updated(label);
                }
            }
        }

        Ok(())
    }

    pub fn get_ordered_labels(&self) -> Vec<Label> {
        let mut labels = self.labels.values().cloned().collect::<Vec<_>>();

        labels.sort_by(|l1, l2| -> Ordering {
            if l1.label_type == l2.label_type {
                return l1.order.cmp(&l2.order);
            }

            if (l1.label_type as i32) < (l2.label_type as i32) {
                return Ordering::Greater;
            }

            Ordering::Less
        });

        labels
    }
}

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();
