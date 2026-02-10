use crate::datatypes::labels::color_to_display;
use crate::datatypes::{MovableSystemFolder, SystemLabelId};
use crate::{
    AppError,
    datatypes::labels::hierarchy::{self, Hierarchy},
};
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::{LabelColor, LabelType, LocalLabelId, SystemLabel};
use proton_core_common::models::{Label, ModelIdExtension};
use stash::orm::Model;
use stash::stash::Tether;

/// This enum represents the action of moving a message or conversation to a folder.
///
#[derive(Debug, Clone, PartialEq)]
pub enum MoveAction {
    /// Move to a system folder (e.g. Inbox, Sent, Archive, Trash).
    SystemFolder(MovableSystemFolderAction),

    /// Move to a custom folder.
    CustomFolder(CustomFolderAction),
}

impl MoveAction {
    /// Create a vector of `MoveAction` from a vector of `Label`.
    /// It is meant to be called for each item for which action is calculated.
    /// After which all those vectors joined together should be passed to `finalize` method.
    /// In order to properly calculate the `is_selected` field.
    ///
    /// # Arguments
    ///
    /// * `iter` - An iterator over the labels. Expected to be sorted by `display_order`.
    ///
    pub fn vec<'a>(iter: impl IntoIterator<Item = &'a Label>) -> Vec<Self> {
        iter.into_iter()
            .filter_map(|label| match label.label_type {
                LabelType::System => Some(MoveAction::SystemFolder(
                    MovableSystemFolderAction::from_label(label)?,
                )),

                LabelType::Folder => Some(MoveAction::CustomFolder(
                    CustomFolderAction::from_label(label)?,
                )),
                _ => None,
            })
            .collect()
    }

    /// Method utilizes map to calculate the final state of the label.
    /// It requires all the duplicated labels to be present from the `vec` method.
    /// Besides that it also calculates the color of the custom folders
    /// and builds their folder structure.
    ///
    /// # Arguments
    ///
    /// * `actions` - An iterator over the actions. Duplicates for each item are expected.
    /// * `interface` - An interface that is used to load the labels.
    ///
    pub async fn finalize(
        actions: impl IntoIterator<Item = MoveAction>,
        tether: &Tether,
    ) -> Result<Vec<MoveAction>, AppError> {
        let actions = MoveAction::calculate_color(actions, tether).await?;
        let actions = MoveAction::build_folder_structure(actions);

        Ok(actions.collect())
    }

    /// Method analogical to finalize, but it only operates on system labels.
    /// So it does not calculate the color or build the folder structure.
    /// It does however calculate the final state of the label as selection status.
    /// It is especially useful when dealing with the messages.
    /// Messages in Conversation context may be scattered across multiple folders.
    ///
    /// # Arguments
    ///
    /// * `actions` - An iterator over the actions. Duplicates for each item are expected.
    ///
    pub fn system(actions: impl IntoIterator<Item = MoveAction>) -> Vec<MovableSystemFolderAction> {
        actions
            .into_iter()
            .filter_map(|action| match action {
                MoveAction::SystemFolder(action) => Some(action),
                _ => None,
            })
            .collect()
    }

    /// Method for building the folder structure.
    /// It utilizes the hierarchy module to build the folder structure.
    ///
    fn build_folder_structure(
        actions: impl IntoIterator<Item = MoveAction>,
    ) -> impl Iterator<Item = MoveAction> {
        let actions = actions.into_iter();
        let system_size = SystemLabel::movable_folders().len();
        let (custom_size, _) = actions.size_hint();
        let (system_folders, custom_folders) = actions.fold(
            (
                Vec::with_capacity(system_size),
                Vec::with_capacity(custom_size),
            ),
            |(mut system, mut custom), action| {
                match action {
                    MoveAction::SystemFolder(action) => system.push(action),
                    MoveAction::CustomFolder(action) => custom.push(action),
                }

                (system, custom)
            },
        );

        let custom_folders = hierarchy::custom_folder_hierarchy(&custom_folders)
            .into_iter()
            .map(MoveAction::CustomFolder);

        system_folders
            .into_iter()
            .map(MoveAction::SystemFolder)
            .chain(custom_folders)
    }

    /// Method for calculating the color of the custom folders.
    /// Color is calculated based on user settings.
    /// Method requires access to the database for loading the settings & labels.
    ///
    async fn calculate_color(
        actions: impl IntoIterator<Item = MoveAction>,
        tether: &Tether,
    ) -> Result<Vec<MoveAction>, AppError> {
        use futures::stream::{self, StreamExt, TryStreamExt};

        let actions: Vec<MoveAction> = stream::iter(actions.into_iter())
            .then(|action| async move {
                match action {
                    MoveAction::CustomFolder(mut action) => {
                        action.color = color_to_display(
                            &Label::load(action.local_id, tether).await?.unwrap(),
                            tether,
                        )
                        .await?;

                        Ok::<_, AppError>(MoveAction::CustomFolder(action))
                    }
                    MoveAction::SystemFolder(action) => Ok(MoveAction::SystemFolder(action)),
                }
            })
            .try_collect()
            .await?;

        Ok(actions)
    }
}

/// This struct represents a system folder that can be used as an action.
///
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct MovableSystemFolderAction {
    /// The database id of the label.
    pub local_id: LocalLabelId,

    /// The name of the system folder embedded as finite enum list.
    pub name: MovableSystemFolder,
}

impl MovableSystemFolderAction {
    pub(crate) fn from_label(label: &Label) -> Option<Self> {
        Some(Self {
            local_id: label.local_id?,
            name: MovableSystemFolder::new(label)?,
        })
    }

    pub(crate) async fn inbox(tether: &Tether) -> Result<Self, AppError> {
        let local_id = Label::remote_id_counterpart(LabelId::inbox(), tether)
            .await?
            .expect("Should be set");
        Ok(Self {
            local_id,
            name: MovableSystemFolder::Inbox,
        })
    }

    pub(crate) async fn archive(tether: &Tether) -> Result<Self, AppError> {
        let local_id = Label::remote_id_counterpart(LabelId::archive(), tether)
            .await?
            .expect("Should be set");
        Ok(Self {
            local_id,
            name: MovableSystemFolder::Archive,
        })
    }

    pub(crate) async fn trash(tether: &Tether) -> Result<Self, AppError> {
        let local_id = Label::remote_id_counterpart(LabelId::trash(), tether)
            .await?
            .expect("Should be set");
        Ok(Self {
            local_id,
            name: MovableSystemFolder::Trash,
        })
    }

    pub(crate) async fn spam(tether: &Tether) -> Result<Self, AppError> {
        let local_id = Label::remote_id_counterpart(LabelId::spam(), tether)
            .await?
            .expect("Should be set");
        Ok(Self {
            local_id,
            name: MovableSystemFolder::Spam,
        })
    }
}

/// This struct represents a custom folder that can be used as an action.
///
#[derive(Debug, Clone, PartialEq)]
pub struct CustomFolderAction {
    /// The database id of the label.
    pub local_id: LocalLabelId,

    /// The name of the folder.
    pub name: String,

    /// Folder color is calculated based on user settings.
    pub color: Option<LabelColor>,

    /// The order in which the folder should be displayed.
    pub display_order: u32,

    /// The parent folder of the current folder.
    pub parent: Option<LocalLabelId>,

    /// It holds folder structure as self reference within vector.
    pub children: Vec<CustomFolderAction>,
}

impl CustomFolderAction {
    fn from_label(label: &Label) -> Option<Self> {
        Some(Self {
            local_id: label.local_id?,
            name: label.name.clone(),
            color: None,
            parent: label.local_parent_id,
            display_order: label.display_order,
            children: vec![],
        })
    }
}

impl Default for CustomFolderAction {
    fn default() -> Self {
        Self {
            local_id: LocalLabelId::from(0),
            name: String::default(),
            color: None,
            display_order: 0,
            parent: None,
            children: vec![],
        }
    }
}

impl Hierarchy for CustomFolderAction {
    fn local_id(&self) -> u64 {
        self.local_id.as_u64()
    }

    fn parent_id(&self) -> Option<u64> {
        self.parent.map(|x| x.as_u64())
    }

    fn set_children(&mut self, children: Vec<Self>) {
        self.children = children;
    }

    fn display_order(&self) -> u32 {
        self.display_order
    }
}
