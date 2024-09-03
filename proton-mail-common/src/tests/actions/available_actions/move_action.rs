use super::{CustomFolderAction, MoveAction, SystemFolderAction};
use crate::datatypes::{LabelType, SystemLabel};
use crate::models::Label;
use crate::{label, lid, rid};
use test_case::test_case;

#[test_case(&[], |_| false, &[]; "TEST1: empty")]
#[test_case(&[label!(local_id: lid!(0), remote_id: rid!("0"), label_type: LabelType::System)], |_| false, &[
    MoveAction::SystemFolder(SystemFolderAction {
        local_id: 0.into(),
        name: SystemLabel::Inbox,
        is_selected: Some(false)
    })
]; "TEST2: single system folder, not selected")]
#[test_case(&[label!(local_id: lid!(0), remote_id: rid!("0"), label_type: LabelType::System)], |_| true, &[
    MoveAction::SystemFolder(SystemFolderAction {
        local_id: 0.into(),
        name: SystemLabel::Inbox,
        is_selected: Some(true)
    })
]; "TEST3: single system folder, selected")]
#[test_case(
    &[
        label!(local_id: lid!(0), remote_id: rid!("0"), label_type: LabelType::System),
        label!(local_id: lid!(0), remote_id: rid!("0"), label_type: LabelType::System),
    ],
    |_| true,
    &[MoveAction::SystemFolder(SystemFolderAction {
        local_id: 0.into(),
        name: SystemLabel::Inbox,
        is_selected: Some(true)
    })]; "TEST4: all system folder selected")]
#[test_case(
        &[
            label!(local_id: lid!(0), remote_id: rid!("0"), label_type: LabelType::System),
            label!(local_id: lid!(0), remote_id: rid!("0"), label_type: LabelType::System),
        ],
        |_| false,
        &[MoveAction::SystemFolder(SystemFolderAction {
            local_id: 0.into(),
            name: SystemLabel::Inbox,
            is_selected: Some(false)
        })]; "TEST5: none system folder selected")]
#[test_case(
    &[
        label!(local_id: lid!(0), remote_id: rid!("0"), label_type: LabelType::System),
        label!(local_id: lid!(0), name: format!("name"), remote_id: rid!("0"), label_type: LabelType::System),
    ],
    |label| label.name.as_str() == "name",
    &[MoveAction::SystemFolder(SystemFolderAction {
        local_id: 0.into(),
        name: SystemLabel::Inbox,
        is_selected: None
    })]; "TEST6: system folder partially selected")]
#[test_case(
    &[
        label!(local_id: lid!(0), remote_id: rid!("0"), label_type: LabelType::Folder),
        label!(local_id: lid!(0), name: format!("name"), remote_id: rid!("0"), label_type: LabelType::Folder),
    ],
    |label| label.name.as_str() == "name",
    &[MoveAction::CustomFolder(CustomFolderAction {
        local_id: 0.into(),
        name: "name".into(),
        color: Default::default(),
        parent: None,
        is_selected: None
    })]; "TEST7: custom folder partially selected")]
#[test_case(
    &[
        label!(local_id: lid!(0), remote_id: rid!("0"), label_type: LabelType::Folder),
        label!(local_id: lid!(1), name: format!("name"), local_parent_id: lid!(0), remote_id: rid!("1"), label_type: LabelType::Folder),
    ],
    |label| label.name.as_str() == "name",
    &[MoveAction::CustomFolder(CustomFolderAction {
        local_id: 0.into(),
        name: Default::default(),
        color: Default::default(),
        parent: None,
        is_selected: Some(false),
    }), MoveAction::CustomFolder(CustomFolderAction {
        local_id: 1.into(),
        name: "name".into(),
        color: Default::default(),
        parent: Some(0.into()),
        is_selected: Some(true),
    })]; "TEST8: custom folder selected with parent")]
pub fn test_is_selected(
    labels: &[Label],
    is_selected: impl Fn(&Label) -> bool,
    expected: &[MoveAction],
) {
    let result = MoveAction::finalize(MoveAction::vec(labels, is_selected));

    assert_eq!(result, expected);
}
