use crate::AppError;
use crate::datatypes::labels::{color_to_display, messages_counts};
use crate::datatypes::{LabelColor, LabelDescription};
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::Label;
use stash::orm::Model;
use stash::stash::Tether;

/// Contextual representation of a `Label` when it is opened for display.
#[derive(Clone, Debug)]
pub struct CustomFolder {
    /// Local id of the Label.
    pub local_id: LocalLabelId,

    /// TODO: Document this field.
    pub parent_id: Option<LocalLabelId>,

    /// List of the Labels contained in this Folder
    pub children: Vec<CustomFolder>,

    /// Color to display the Folder with.
    pub color: Option<LabelColor>,

    /// Description of the Folder.
    pub description: LabelDescription,

    /// TODO: Document this field.
    pub display: bool,

    /// Is the folder expanded?
    pub expanded: bool,

    /// Name of the Folder.
    pub name: String,

    /// TODO: Document this field.
    pub notify: bool,

    /// Order to display the Folders.
    pub display_order: u32,

    /// TODO: Document this field.
    pub path: Option<String>,

    /// TODO: Document this field.
    pub sticky: bool,

    /// Total number of Messages in this Folder.
    pub total: u64,

    /// Number of unread Messages in this Folder.
    pub unread: u64,
}

impl CustomFolder {
    /// Create a new `CustomFolder`.
    ///
    /// Create a view on a [`Label`] keeping and transforming the field, so they contain the data
    /// needed by UI.
    ///
    /// Note: The field `children` is created empty, if needed it must be filled later.
    ///
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

    /// Create a vec of `CustomFolder` from a vec of [`Label`]
    ///
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
