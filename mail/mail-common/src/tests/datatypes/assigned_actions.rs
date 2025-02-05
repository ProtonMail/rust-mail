use crate::datatypes::SwipeActionMoveToTarget;
use crate::datatypes::{AssignedSwipeAction, SwipeAction};
use pretty_assertions::assert_eq;
use proton_core_common::datatypes::SystemLabel;
use proton_mail_test_utils::test_context::MailTestContext;
use test_case::test_case;

#[test_case(SwipeAction::NoAction, AssignedSwipeAction::NoAction)]
#[test_case(SwipeAction::Star, AssignedSwipeAction::ToggleStar)]
#[test_case(SwipeAction::MarkAsRead, AssignedSwipeAction::ToggleRead)]
// LabelAs action means, that the ui has to display an additional popup with the list of labels
#[test_case(SwipeAction::LabelAs, AssignedSwipeAction::LabelAs)]
// MoveTo action means, that the ui has to display an additional popup with the list of folders
#[test_case(
    SwipeAction::MoveTo,
    AssignedSwipeAction::MoveTo(SwipeActionMoveToTarget::UnknownLabel)
)]
#[tokio::test]
async fn when_swipe_action_doesnt_require_any_extra_context(
    swipe_action: SwipeAction,
    expected_action: AssignedSwipeAction,
) {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection();

    let actual = AssignedSwipeAction::load(swipe_action, &tether)
        .await
        .expect("Action");

    assert_eq!(expected_action, actual);
}

#[test_case(SwipeAction::Trash, SystemLabel::Trash)]
#[test_case(SwipeAction::Spam, SystemLabel::Spam)]
#[test_case(SwipeAction::Archive, SystemLabel::Archive)]
#[tokio::test]
async fn when_it_is_move_to_system_folder_action(
    swipe_action: SwipeAction,
    expected_system_label: SystemLabel,
) {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection();

    let actual = AssignedSwipeAction::load(swipe_action, &tether)
        .await
        .expect("Action");

    let local_id = expected_system_label
        .local_id(&tether)
        .await
        .expect("Local id")
        .expect("Local id");

    assert_eq!(
        AssignedSwipeAction::MoveTo(SwipeActionMoveToTarget::SystemLabel {
            label: expected_system_label,
            id: local_id,
        }),
        actual
    );
}
