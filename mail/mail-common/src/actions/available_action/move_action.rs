#[cfg(test)]
#[path = "../../tests/actions/available_actions/move_action.rs"]
mod tests;

use crate::datatypes::labels::color_to_display;
use crate::datatypes::{MovableSystemFolder, SystemLabelId};
use crate::{
    datatypes::{
        labels::hierarchy::{self, Hierarchy},
        LabelColor, LabelType, SystemLabel,
    },
    models::Label,
    AppError,
};
use proton_core_common::datatypes::{Id, LabelId, LocalId, RemoteId};
use stash::orm::Model;
use stash::stash::{AgnosticInterface, Interface};
use std::collections::BTreeMap;
use std::iter::once;

/// This enum represents the action of moving a message or conversation to a folder.
///
#[derive(Debug, Clone, PartialEq)]
pub enum MoveAction {
    /// Move to a sysem folder (e.g. Inbox, Sent, Archive, Trash).
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
    /// * `is_selected` - A function that determines if the label is selected for the given item.
    ///
    pub fn vec<'a>(
        iter: impl IntoIterator<Item = &'a Label>,
        is_selected: impl Fn(&Label) -> bool,
    ) -> Vec<Self> {
        iter.into_iter()
            .filter_map(|label| match label.label_type {
                LabelType::System => Some(MoveAction::SystemFolder(
                    MovableSystemFolderAction::from_label(label, is_selected(label))?,
                )),

                LabelType::Folder => Some(MoveAction::CustomFolder(CustomFolderAction {
                    local_id: label.local_id?,
                    name: label.name.clone(),
                    color: None,
                    parent: label.local_parent_id,
                    display_order: label.display_order,
                    children: vec![],
                    is_selected: Some(is_selected(label)),
                })),
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
    pub async fn finalize<A>(
        actions: impl IntoIterator<Item = MoveAction>,
        interface: &A,
    ) -> Result<Vec<MoveAction>, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let actions = MoveAction::calculate_selection(actions);
        let actions = MoveAction::calculate_color(actions, interface).await?;
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
        MoveAction::calculate_selection(actions)
            .filter_map(|action| match action {
                MoveAction::SystemFolder(action) => Some(action),
                _ => None,
            })
            .collect()
    }

    /// Method for calculating the selection status of the labels.
    /// It evaluates all the duplicated labels and their selection status from each item.
    ///
    pub(super) fn calculate_selection(
        actions: impl IntoIterator<Item = MoveAction>,
    ) -> impl Iterator<Item = MoveAction> {
        let mut map = MoveActionMap::new();

        for action in actions {
            match &action {
                MoveAction::SystemFolder(system_action) => {
                    map.insert(system_action.local_id, action);
                }
                MoveAction::CustomFolder(system_action) => {
                    map.insert(system_action.local_id, action);
                }
            }
        }

        map.drain()
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
    async fn calculate_color<A>(
        actions: impl IntoIterator<Item = MoveAction>,
        interface: &A,
    ) -> Result<Vec<MoveAction>, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        use futures::stream::{self, StreamExt, TryStreamExt};

        let actions: Vec<MoveAction> = stream::iter(actions.into_iter())
            .then(|action| async move {
                match action {
                    MoveAction::CustomFolder(mut action) => {
                        action.color = color_to_display(
                            &Label::load(action.local_id, interface).await?.unwrap(),
                            interface,
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

    fn is_selected(&self) -> Option<bool> {
        match self {
            MoveAction::SystemFolder(action) => action.is_selected,
            MoveAction::CustomFolder(action) => action.is_selected,
        }
    }

    fn set_selected(&mut self, selected: Option<bool>) {
        match self {
            MoveAction::SystemFolder(action) => action.is_selected = selected,
            MoveAction::CustomFolder(action) => action.is_selected = selected,
        }
    }
}

/// This struct represents a system folder that can be used as an action.
///
#[derive(Debug, Clone, PartialEq)]
pub struct SystemFolderAction {
    /// The database id of the label.
    pub local_id: LocalId,

    /// The name of the system folder embedded as finite enum list.
    pub name: SystemLabel,

    /// This field is used to determine if the folder is selected or not
    /// for given list of messages or conversations.
    ///
    /// Option<bool> is used to represent three states:
    /// * Some(true) - All the folder occurrences across all messages/conversations have them assigned.
    /// * Some(false) - None of the folder occurrences across all messages/conversations have them assigned.
    /// * None - Some of the folder occurrences across all messages/conversations have them assigned and some don't.
    ///
    /// Option type was chosen over dedicated enum to make it easier to calculate the final state of the folder.
    /// Due to the fact algorithm calculate this value multiple times and then modify already existing fields.
    pub is_selected: Option<bool>,
}

/// This struct represents a system folder that can be used as an action.
///
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct MovableSystemFolderAction {
    /// The database id of the label.
    pub local_id: LocalId,

    /// The name of the system folder embedded as finite enum list.
    pub name: MovableSystemFolder,

    /// This field is used to determine if the folder is selected or not
    /// for given list of messages or conversations.
    ///
    /// Option<bool> is used to represent three states:
    /// * Some(true) - All the folder occurrences across all messages/conversations have them assigned.
    /// * Some(false) - None of the folder occurrences across all messages/conversations have them assigned.
    /// * None - Some of the folder occurrences across all messages/conversations have them assigned and some don't.
    ///
    /// Option type was chosen over dedicated enum to make it easier to calculate the final state of the folder.
    /// Due to the fact algorithm calculate this value multiple times and then modify already existing fields.
    pub is_selected: Option<bool>,
}

impl MovableSystemFolderAction {
    pub(crate) fn from_label(label: &Label, is_selected: bool) -> Option<Self> {
        Some(Self {
            local_id: label.local_id?,
            name: MovableSystemFolder::new(label)?,
            is_selected: Some(is_selected),
        })
    }

    pub(crate) async fn inbox<A>(interface: &A) -> Result<Self, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let local_id = RemoteId::counterpart::<Label, _>(&LabelId::inbox().into_inner(), interface)
            .await?
            .expect("Should be set");
        Ok(Self {
            local_id,
            name: MovableSystemFolder::Inbox,
            is_selected: Some(false),
        })
    }

    pub(crate) async fn archive<A>(interface: &A) -> Result<Self, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let local_id =
            RemoteId::counterpart::<Label, _>(&LabelId::archive().into_inner(), interface)
                .await?
                .expect("Should be set");
        Ok(Self {
            local_id,
            name: MovableSystemFolder::Archive,
            is_selected: Some(false),
        })
    }

    pub(crate) async fn trash<A>(interface: &A) -> Result<Self, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let local_id = RemoteId::counterpart::<Label, _>(&LabelId::trash().into_inner(), interface)
            .await?
            .expect("Should be set");
        Ok(Self {
            local_id,
            name: MovableSystemFolder::Trash,
            is_selected: Some(false),
        })
    }

    pub(crate) async fn spam<A>(interface: &A) -> Result<Self, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let local_id = RemoteId::counterpart::<Label, _>(&LabelId::spam().into_inner(), interface)
            .await?
            .expect("Should be set");
        Ok(Self {
            local_id,
            name: MovableSystemFolder::Spam,
            is_selected: Some(false),
        })
    }
}

/// This struct represents a custom folder that can be used as an action.
///
#[derive(Debug, Clone, PartialEq)]
pub struct CustomFolderAction {
    /// The database id of the label.
    pub local_id: LocalId,

    /// The name of the folder.
    pub name: String,

    /// Folder color is calculated based on user settings.
    pub color: Option<LabelColor>,

    /// The order in which the folder should be displayed.
    pub display_order: u32,

    /// The parent folder of the current folder.
    pub parent: Option<LocalId>,

    /// It holds folder structure as self reference within vector.
    pub children: Vec<CustomFolderAction>,

    /// This field is used to determine if the folder is selected or not
    /// for given list of messages or conversations.
    ///
    /// For more information check the documentation of analaogical field in [SystemFolderAction].
    pub is_selected: Option<bool>,
}

impl Default for CustomFolderAction {
    fn default() -> Self {
        Self {
            local_id: LocalId::from(0),
            name: String::default(),
            color: None,
            display_order: 0,
            parent: None,
            children: vec![],
            is_selected: None,
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

struct MoveActionMap {
    map: BTreeMap<LocalId, Vec<MoveAction>>,
}

impl MoveActionMap {
    fn new() -> Self {
        Self {
            map: BTreeMap::new(),
        }
    }

    fn insert(&mut self, label_id: LocalId, action: MoveAction) {
        self.map.entry(label_id).or_default().push(action);
    }

    fn drain(self) -> impl Iterator<Item = MoveAction> {
        self.map.into_iter().filter_map(|(_, mut actions)| {
            if actions.is_empty() {
                return None;
            }

            let is_selected = actions.iter().all(|x| x.is_selected().unwrap_or(false));

            if is_selected {
                actions.pop()
            } else {
                let is_partially_selected =
                    actions.iter().any(|x| x.is_selected().unwrap_or(false));
                let mut action = actions.pop()?;

                if is_partially_selected {
                    action.set_selected(None);
                } else {
                    action.set_selected(Some(false))
                }

                Some(action)
            }
        })
    }
}

/// Represent all the actions to move a message.
/// Either move to a system folder or open a dialog to choose a custom folder.
///
#[derive(Debug, Clone, PartialEq)]
pub enum RealMoveItemAction {
    MoveToSystemFolder(MovableSystemFolderAction),
    MoveTo,
}

impl RealMoveItemAction {
    pub(crate) fn from_actions(actions: Vec<MovableSystemFolderAction>) -> Vec<Self> {
        actions
            .into_iter()
            .map(RealMoveItemAction::from)
            .chain(once(Self::MoveTo))
            .collect()
    }
}

impl From<MovableSystemFolderAction> for RealMoveItemAction {
    fn from(value: MovableSystemFolderAction) -> Self {
        Self::MoveToSystemFolder(value)
    }
}
