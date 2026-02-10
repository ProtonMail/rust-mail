use crate::datatypes::SwipeActionMoveToTarget;
use crate::datatypes::SystemLabelId;
use crate::datatypes::{AssignedSwipeAction, SwipeAction};
use pretty_assertions::assert_eq;
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::SystemLabel;
use proton_core_common::models::Label;
use proton_mail_common::test_utils::test_context::MailTestContext;
use test_case::test_case;

#[test_case(SwipeAction::NoAction, LabelId::inbox(), AssignedSwipeAction::NoAction)]
#[test_case(SwipeAction::Star, LabelId::inbox(), AssignedSwipeAction::ToggleStar)]
#[test_case(
    SwipeAction::MarkAsRead,
    LabelId::inbox(),
    AssignedSwipeAction::ToggleRead
)]
// LabelAs action means, that the ui has to display an additional popup with the list of labels
#[test_case(SwipeAction::LabelAs, LabelId::inbox(), AssignedSwipeAction::LabelAs)]
// MoveTo action means, that the ui has to display an additional popup with the list of folders
#[test_case(
    SwipeAction::MoveTo,
    LabelId::inbox(),
    AssignedSwipeAction::MoveTo(SwipeActionMoveToTarget::MoveToUnknownLabel)
)]
// If user is already in target folder, do nothing
#[test_case(SwipeAction::Trash, LabelId::trash(), AssignedSwipeAction::NoAction)]
#[test_case(SwipeAction::Spam, LabelId::spam(), AssignedSwipeAction::NoAction)]
#[test_case(
    SwipeAction::Archive,
    LabelId::archive(),
    AssignedSwipeAction::NoAction
)]
#[tokio::test]
async fn when_swipe_action_doesnt_require_any_extra_context(
    swipe_action: SwipeAction,
    current_label: LabelId,
    expected_action: AssignedSwipeAction,
) {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();
    let current_label = Label::resolve_local_label_id(current_label, &tether)
        .await
        .expect("current id");

    let actual = AssignedSwipeAction::load(swipe_action, current_label, &tether)
        .await
        .expect("Action");

    assert_eq!(expected_action, actual);
}

#[test_case(SwipeAction::Trash, LabelId::inbox(), SystemLabel::Trash)]
#[test_case(SwipeAction::Spam, LabelId::inbox(), SystemLabel::Spam)]
#[test_case(SwipeAction::Archive, LabelId::inbox(), SystemLabel::Archive)]
#[test_case(SwipeAction::Trash, LabelId::all_mail(), SystemLabel::Trash)]
#[test_case(SwipeAction::Spam, LabelId::all_mail(), SystemLabel::Spam)]
#[test_case(SwipeAction::Archive, LabelId::all_mail(), SystemLabel::Archive)]
#[tokio::test]
async fn when_it_is_move_to_system_folder_action(
    swipe_action: SwipeAction,
    current_label: LabelId,
    expected_system_label: SystemLabel,
) {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let current_label = Label::resolve_local_label_id(current_label, &tether)
        .await
        .expect("current id");

    let actual = AssignedSwipeAction::load(swipe_action, current_label, &tether)
        .await
        .expect("Action");

    let local_id = expected_system_label
        .local_id(&tether)
        .await
        .expect("Local id")
        .expect("Local id");

    assert_eq!(
        AssignedSwipeAction::MoveTo(SwipeActionMoveToTarget::MoveToSystemLabel {
            label: expected_system_label,
            id: local_id,
        }),
        actual
    );
}
