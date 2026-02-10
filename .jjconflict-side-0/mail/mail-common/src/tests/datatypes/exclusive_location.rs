use super::{ExclusiveLocation, SystemLabel};
use crate::{
    datatypes::LabelType::{self, *},
    datatypes::SystemLabelId,
};
use proton_core_api::services::proton::LabelId;
use proton_core_common::models::Label;
use test_case::test_case;

#[test_case(&[] => None; "TEST1 - empty")]
#[test_case(&[(&SystemLabel::Inbox.into(), System )] => Some(ExclusiveLocation::System { name: SystemLabel::Inbox, local_id: 0.into()}); "TEST2 - only inbox"
)]
#[test_case(&[
        (&SystemLabel::Snoozed.into(), System),
        (&SystemLabel::AlmostAllMail.into(), System),
        (&SystemLabel::Scheduled.into(), System),
        (&SystemLabel::Starred.into(), System),
        (&SystemLabel::Outbox.into(), System),
        (&SystemLabel::Drafts.into(), System),
        (&SystemLabel::Sent.into(), System),
        (&SystemLabel::Archive.into(), System),
        (&SystemLabel::AllMail.into(), System),
        (&SystemLabel::Spam.into(), System),
        (&SystemLabel::Trash.into(), System),
        (&SystemLabel::AllSent.into(), System),
        (&SystemLabel::AllDrafts.into(), System),
        (&SystemLabel::Inbox.into(), System),
    ] => Some(ExclusiveLocation::System { name: SystemLabel::Inbox, local_id: 13.into() }); "TEST3 - all system labels"
)]
#[test_case(
    &[
        (&SystemLabel::Outbox.into(), System),
        (&SystemLabel::Trash.into(), System)
    ] => Some(ExclusiveLocation::System { name: SystemLabel::Trash, local_id: 1.into() }); "TEST4 - outbox and trash"
)]
#[test_case(
    &[
        (&SystemLabel::Inbox.into(), System),
        (&SystemLabel::Outbox.into(), System)
] => Some(ExclusiveLocation::System { name: SystemLabel::Inbox, local_id: 0.into() }); "TEST5 - message sent to themself"
)]
#[test_case(&[(&LabelId::starred(), System)]
    => None; "TEST6 - message is starred and does not belong to any exclusive location"
)]
#[test_case(&[(&LabelId::from("custom_folder"), Folder)]
    => Some(ExclusiveLocation::Custom {
        name: "custom_folder".to_string(),
        local_id: 0.into(),
        color: Default::default()
    }); "TEST7 - in custom folder"
)]
#[test_case(&[(&LabelId::from("custom_folder"), Label)] => None; "TEST8 - in custom folder but label is not folder"
)]
#[test_case(&[(&LabelId::from("custom_folder"), ContactGroup)] => None; "TEST9 - in custom folder but label is not folder"
)]
#[test_case(&[(&LabelId::from("custom_folder"), System)] => None; "TEST10 - in custom folder but label is not folder"
)]
#[test_case(&[
        (&LabelId::starred(), System),
        (&LabelId::from("custom_folder"), Folder)
    ]
    => Some(ExclusiveLocation::Custom {
        name: "custom_folder".to_string(),
        local_id: 1.into(),
        color: Default::default()
    }); "TEST11 - in custom folder and starred"
)]
// There should never be such a case to have a message or conversation
// in two custom folders but in a case there are two of them defined
// it is definetly a bug! We return None and log an appropriate error.
#[test_case(&[
        (&LabelId::from("first_custom_folder"), Folder),
        (&LabelId::from("second_custom_folder"), Folder)
    ]
    => None; "TEST12 - in two custom folders"
)]
#[test_case(&[
        (&LabelId::from("custom_folder"), Folder),
        (&SystemLabel::Inbox.into(), System)
    ]
    => Some(ExclusiveLocation::System { name: SystemLabel::Inbox, local_id: 1.into() }); "TEST13 - in custom folder and inbox"
)]
#[test_case(&[
        (&LabelId::starred(), System),
        (&LabelId::from("label"), Label),
        (&LabelId::from("contact_group"), ContactGroup),
        (&LabelId::from("custom_folder"), Folder),
    ]
    => Some(ExclusiveLocation::Custom {
        name: "custom_folder".to_string(),
        local_id: 3.into(),
        color: Default::default()
    }); "TEST14 - all possible label types, but system label is not exclusive"
)]
fn test_exclusive_location(labels: &[(&LabelId, LabelType)]) -> Option<ExclusiveLocation> {
    let labels = labels_from_ids(labels);

    ExclusiveLocation::from_labels(&labels)
}

fn labels_from_ids(labels: &[(&LabelId, LabelType)]) -> Vec<Label> {
    labels
        .iter()
        .enumerate()
        .map(|(idx, (rid, label_type))| Label {
            remote_id: Some((*rid).clone()),
            local_id: Some((idx as u64).into()),
            name: rid.as_str().to_owned(),
            label_type: *label_type,
            ..Label::test_default()
        })
        .collect()
}
