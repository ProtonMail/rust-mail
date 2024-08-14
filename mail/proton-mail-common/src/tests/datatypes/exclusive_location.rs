#![allow(non_snake_case)]

use super::*;
use crate::{
    datatypes::LabelType::{self, *},
    models::Label,
};
use proton_core_common::datatypes::LabelId;
use test_case::test_case;

#[test_case(&[] => None; "TEST1 - empty")]
#[test_case(&[( &*INBOX_LABEL_ID, System )] => Some(ExclusiveLocation::Inbox); "TEST2 - only inbox")]
#[test_case(&[
        (&LabelId::snoozed(), System),
        (&LabelId::almost_all_mail(), System),
        (&LabelId::all_scheduled(), System),
        (&LabelId::starred(), System),
        (&LabelId::outbox(), System),
        (&LabelId::drafts(), System),
        (&LabelId::sent(), System),
        (&LabelId::archive(), System),
        (&LabelId::all_mail(), System),
        (&LabelId::spam(), System),
        (&LabelId::trash(), System),
        (&LabelId::all_sent(), System),
        (&LabelId::all_drafts(), System),
        (&LabelId::inbox(), System),
    ] => Some(ExclusiveLocation::Inbox); "TEST3 - all system labels"
)]
#[test_case(
    &[
        (&*OUTBOX_LABEL_ID, System),
        (&*TRASH_LABEL_ID, System)
    ] => Some(ExclusiveLocation::Trash); "TEST4 - outbox and trash"
)]
#[test_case(
    &[
        (&*INBOX_LABEL_ID, System),
        (&*OUTBOX_LABEL_ID, System)
] => Some(ExclusiveLocation::Inbox); "TEST5 - message sent to themself")]
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
#[test_case(&[(&LabelId::from("custom_folder"), Label)] => None; "TEST8 - in custom folder but label is not folder")]
#[test_case(&[(&LabelId::from("custom_folder"), ContactGroup)] => None; "TEST9 - in custom folder but label is not folder")]
#[test_case(&[(&LabelId::from("custom_folder"), System)] => None; "TEST10 - in custom folder but label is not folder")]
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
        (&INBOX_LABEL_ID, System)
    ]
    => Some(ExclusiveLocation::Inbox); "TEST13 - in custom folder and inbox"
)]
#[test_case(&[
        (&LabelId::drafts(), System),
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
            ..Default::default()
        })
        .collect()
}
