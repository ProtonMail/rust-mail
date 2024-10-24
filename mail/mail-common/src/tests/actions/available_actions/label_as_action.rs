use super::LabelAsAction;
use crate as proton_mail_common;
use crate::models::Label;
use proton_mail_test_utils::{label, lid};
use test_case::test_case;

#[test_case(&[], |_| false, &[]; "TEST1: empty")]
#[test_case(&[label!(local_id: lid!(0))], |_| false, &[
    LabelAsAction {
        label_id: 0.into(),
        name: Default::default(),
        color: Default::default(),
        is_selected: Some(false)
    }
]; "TEST2: single label, not selected")]
#[test_case(&[label!(local_id: lid!(0))], |_| true, &[
    LabelAsAction {
        label_id: 0.into(),
        name: Default::default(),
        color: Default::default(),
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
        is_selected: Some(true),
}]; "TEST4: all selected")]
#[test_case(
        &[label!(local_id: lid!(0)), label!(local_id: lid!(0))],
        |_| false,
        &[LabelAsAction {
            label_id: 0.into(),
            name: Default::default(),
            color: Default::default(),
            is_selected: Some(false),
}]; "TEST5: none selected")]
#[test_case(
    &[label!(local_id: lid!(0)), label!(local_id: lid!(0), name: format!("name"))],
    // Function returns selection based on name equality:
    // one is selected while other is not
    |label| label.name.as_str() == "name",
    &[LabelAsAction {
        label_id: 0.into(),
        name: "name".into(),
        color: Default::default(),
        is_selected: None,
}]; "TEST6: partially selected")]
pub fn test_is_selected(
    labels: &[Label],
    is_selected: impl Fn(&Label) -> bool,
    expected: &[LabelAsAction],
) {
    let result = LabelAsAction::finalize(LabelAsAction::vec(labels, is_selected));

    assert_eq!(result, expected);
}
