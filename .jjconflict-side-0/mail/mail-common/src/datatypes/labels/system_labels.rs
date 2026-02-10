use crate::AppError;
use crate::datatypes::LabelDescription;
use crate::datatypes::labels::messages_counts;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::Label;
use stash::orm::Model;
use stash::stash::Tether;

pub struct SystemLabel {
    pub local_id: LocalLabelId,
    pub description: LabelDescription,
    pub display: bool,
    pub name: String,
    pub notify: bool,
    pub display_order: u32,
    pub sticky: bool,
    pub total: u64,
    pub unread: u64,
}

impl SystemLabel {
    pub async fn new(label: &Label, tether: &Tether) -> Result<Self, AppError> {
        let (unread, total) = messages_counts(label, tether).await?;
        let label_description = LabelDescription::new(label);
        Ok(Self {
            local_id: label.id(),
            display: label.display,
            description: label_description,
            name: label.name.clone(),
            notify: label.notify,
            display_order: label.display_order,
            sticky: label.sticky,
            total,
            unread,
        })
    }

    pub async fn from_labels(labels: &[Label], tether: &Tether) -> Result<Vec<Self>, AppError> {
        let mut result = Vec::with_capacity(labels.len());
        for label in labels {
            let label = Self::new(label, tether).await?;
            result.push(label);
        }
        Ok(result)
    }
}
