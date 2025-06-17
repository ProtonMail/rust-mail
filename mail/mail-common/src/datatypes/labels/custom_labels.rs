use crate::AppError;
use crate::datatypes::labels::messages_counts;
use crate::datatypes::{LabelColor, LabelDescription};
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::Label;
use stash::orm::Model;
use stash::stash::Tether;

/// Represent a `Label` defined by End User
pub struct CustomLabel {
    /// Local id of the Label.
    pub local_id: LocalLabelId,

    /// The color of the Label.
    pub color: LabelColor,

    /// Description of this Label.
    pub description: LabelDescription,

    /// TODO: Document this field.
    pub display: bool,

    /// The name of this Label.
    pub name: String,

    /// TODO: Document this field.
    pub notify: bool,

    /// Order to display relative to other `CustomLabel`.
    pub display_order: u32,

    /// TODO: Document this field.
    pub sticky: bool,

    /// Total count of the message in this Label.
    pub total: u64,

    /// Count of unread message in this Label.
    pub unread: u64,
}

impl CustomLabel {
    /// Create a new `CustomLabel`.
    ///
    /// Create a view on a [`Label`] keeping and transforming the field, so they contain the data
    /// needed by UI.
    ///
    pub async fn new(label: &Label, tether: &Tether) -> Result<Self, AppError> {
        let label_description = LabelDescription::new(label);
        let (unread, total) = messages_counts(label, tether).await?;
        Ok(Self {
            local_id: label.id(),
            color: label.color.clone(),
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

    /// Create a vec of `CustomLabel` from a vec of [`Label`]
    ///
    pub async fn from_labels(labels: &[Label], tether: &Tether) -> Result<Vec<Self>, AppError> {
        let mut result = Vec::with_capacity(labels.len());
        for label in labels {
            let label = Self::new(label, tether).await?;
            result.push(label);
        }
        Ok(result)
    }
}
