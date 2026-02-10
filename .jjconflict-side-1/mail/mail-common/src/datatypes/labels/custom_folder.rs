use crate::AppError;
use crate::datatypes::labels::{color_to_display, messages_counts};
use crate::datatypes::{LabelColor, LabelDescription};
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::Label;
use stash::orm::Model;
use stash::stash::Tether;

#[derive(Clone, Debug)]
pub struct CustomFolder {
    pub local_id: LocalLabelId,
    pub parent_id: Option<LocalLabelId>,
    pub children: Vec<CustomFolder>,
    pub color: Option<LabelColor>,
    pub description: LabelDescription,
    pub display: bool,
    pub expanded: bool,
    pub name: String,
    pub notify: bool,
    pub display_order: u32,
    pub path: Option<String>,
    pub sticky: bool,
    pub total: u64,
    pub unread: u64,
}

impl CustomFolder {
    // Note: The field `children` is created empty, if needed it must be filled later.
    pub fn new(
        label: &Label,
        color: Option<LabelColor>,
        unread: u64,
        total: u64,
    ) -> Result<Self, AppError> {
        let label_description = LabelDescription::new(label);
        Ok(Self {
            local_id: label.id(),
            parent_id: label.local_parent_id,
            children: vec![],
            color,
            display: label.display,
            expanded: label.expanded,
            description: label_description,
            name: label.name.clone(),
            notify: label.notify,
            display_order: label.display_order,
            path: label.path.clone(),
            sticky: label.sticky,
            total,
            unread,
        })
    }

    pub async fn from_labels(labels: &[Label], tether: &Tether) -> Result<Vec<Self>, AppError> {
        let mut result = Vec::with_capacity(labels.len());

        for label in labels {
            let color = color_to_display(label, tether).await?;
            let (unread, total) = messages_counts(label, tether).await?;
            let label = Self::new(label, color, unread, total)?;
            result.push(label);
        }

        Ok(result)
    }
}
