use crate::datatypes::SystemLabelId;
use proton_core_api::services::proton::Label as ApiLabel;
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::LabelType;

pub trait ApiLabelTestUtils {
    fn create_api_label(label_name: &str, label_type: LabelType) -> (ApiLabel, LabelId);
    fn get_api_label_with_given_id(label_id: LabelId) -> ApiLabel;
}

impl ApiLabelTestUtils for ApiLabel {
    fn create_api_label(label_name: &str, label_type: LabelType) -> (ApiLabel, LabelId) {
        let label_id = LabelId::from(label_name);
        (
            ApiLabel {
                id: label_id.clone(),
                parent_id: None,
                name: label_name.to_owned(),
                path: None,
                color: String::new(),
                label_type: label_type.into(),
                notify: false,
                display: false,
                sticky: false,
                expanded: false,
                order: 0,
            },
            label_id,
        )
    }

    fn get_api_label_with_given_id(label_id: LabelId) -> ApiLabel {
        let mut label_name = "Default_test_label";
        let mut label_type = LabelType::System;
        match label_id.clone() {
            id if id == SystemLabelId::inbox() => {
                label_name = "Inbox";
            }
            id if id == SystemLabelId::all_drafts() => {
                label_name = "Drafts";
            }
            id if id == SystemLabelId::all_mail() => {
                label_name = "All Mail";
            }
            id if id == SystemLabelId::all_scheduled() => {
                label_name = "Scheduled";
            }
            id if id == SystemLabelId::all_sent() => {
                label_name = "Sent";
            }
            id if id == SystemLabelId::almost_all_mail() => {
                label_name = "Almost All Mail";
            }
            id if id == SystemLabelId::archive() => {
                label_name = "Archive";
            }
            id if id == SystemLabelId::drafts() => {
                label_name = "Drafts";
            }
            id if id == SystemLabelId::inbox() => {
                label_name = "Inbox";
            }
            id if id == SystemLabelId::outbox() => {
                label_name = "Outbox";
            }
            id if id == SystemLabelId::sent() => {
                label_name = "Sent";
            }
            id if id == SystemLabelId::spam() => {
                label_name = "Spam";
            }
            id if id == SystemLabelId::starred() => {
                label_name = "Starred";
            }
            id if id == SystemLabelId::trash() => {
                label_name = "Trash";
            }
            id if id == SystemLabelId::snoozed() => {
                label_name = "Snoozed";
            }

            id if id == SystemLabelId::category_social() => {
                label_name = "Category Social";
                label_type = LabelType::Label;
            }
            id if id == SystemLabelId::category_promotions() => {
                label_name = "Category Promotions";
                label_type = LabelType::Label;
            }
            id if id == SystemLabelId::category_updates() => {
                label_name = "Category Updates";
                label_type = LabelType::Label;
            }
            id if id == SystemLabelId::category_forums() => {
                label_name = "Category Forums";
                label_type = LabelType::Label;
            }
            id if id == SystemLabelId::category_default() => {
                label_name = "Category Default";
                label_type = LabelType::Label;
            }
            _ => {
                label_type = LabelType::Label;
            }
        }
        ApiLabel {
            id: label_id.clone(),
            parent_id: None,
            name: label_name.to_owned(),
            path: None,
            color: String::new(),
            label_type: label_type.into(),
            notify: false,
            display: false,
            sticky: false,
            expanded: false,
            order: 0,
        }
    }
}
