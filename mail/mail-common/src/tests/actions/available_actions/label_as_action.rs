use super::LabelAsAction;
use proton_core_common::models::Label;
use proton_mail_common::label;
use test_case::test_case;

/// Macro wrapping u64 into Option<LocalId> for easier model definition.
macro_rules! lid {
    ($id:expr) => {{
        use proton_core_common::datatypes::LocalLabelId;
        Some(LocalLabelId::from($id))
    }};
}

#[test_case(&[], |_| false, &[]; "TEST1: empty")]
#[test_case(&[label!(local_id: lid!(0))], |_| false, &[
    LabelAsAction {
        label_id: 0.into(),
        name: Default::default(),
        color: Default::default(),
        order: Default::default(),
        is_selected: Some(false)
    }
]; "TEST2: single label, not selected")]
#[test_case(&[label!(local_id: lid!(0))], |_| true, &[
    LabelAsAction {
        label_id: 0.into(),
        name: Default::default(),
        color: Default::default(),
        order: Default::default(),
        is_selected: Some(true)
    }
]; "TEST3: single label, selected")]
#[test_case(
    &[label!(local_id: lid!(0)), label!(local_id: lid!(0))],
    |_| true,
    &[LabelAsAction {
        label_id: 0.into(),
        name: Default::default(),
        color: Default::default(),
        order: Default::default(),
        is_selected: Some(true),
}]; "TEST4: all selected")]
#[test_case(
        &[label!(local_id: lid!(0)), label!(local_id: lid!(0))],
        |_| false,
        &[LabelAsAction {
            label_id: 0.into(),
            name: Default::default(),
            color: Default::default(),
            order: Default::default(),
            is_selected: Some(false),
}]; "TEST5: none selected")]
#[test_case(
    &[label!(local_id: lid!(0)), label!(local_id: lid!(0), name: "name".to_string())],
    // Function returns selection based on name equality:
    // one is selected while other is not
    |label| label.name.as_str() == "name",
    &[LabelAsAction {
        label_id: 0.into(),
        name: "name".into(),
        color: Default::default(),
        order: Default::default(),
        is_selected: None,
}]; "TEST6: partially selected")]
#[test_case(
    &[
        label!(local_id: lid!(1), name: "C".to_string(), display_order: 3),
        label!(local_id: lid!(2), name: "A".to_string(), display_order: 1),
        label!(local_id: lid!(3), name: "B".to_string(), display_order: 2),
    ],
    |_| false,
    &[
        LabelAsAction {
            label_id: 2.into(),
            name: "A".into(),
            color: Default::default(),
            order: 1,
            is_selected: Some(false),
        },
        LabelAsAction {
            label_id: 3.into(),
            name: "B".into(),
            color: Default::default(),
            order: 2,
            is_selected: Some(false),
        },
        LabelAsAction {
            label_id: 1.into(),
            name: "C".into(),
            color: Default::default(),
            order: 3,
            is_selected: Some(false),
        },
    ]; "TEST7: labels are sorted by order")]
pub fn test_is_selected(
    labels: &[Label],
    is_selected: impl Fn(&Label) -> bool,
    expected: &[LabelAsAction],
) {
    let result = LabelAsAction::finalize(LabelAsAction::vec(labels, is_selected));

    assert_eq!(result, expected);
}
