use super::drafts_common::*;
use proton_core_api::services::proton::{LabelId, UserId};
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::draft::recipients::{Recipient, SingleRecipient, ValidationState};
use proton_mail_common::draft::{Draft, DraftActorOptions, RecipientGroupId};
use proton_mail_common::test_utils::message_body::{
    TEST_USER_ID, message_body_test_message_simple, message_body_test_user_secret,
};
use proton_mail_common::test_utils::test_context::MailTestContext;
use proton_mailto::Mailto;

#[tokio::test]
async fn mailto() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();
    let mut message = message_body_test_message_simple();

    message.metadata.label_ids.push(LabelId::drafts());
    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;

    // ---

    let user_ctx = ctx.mail_user_context().await;
    let options = DraftActorOptions::default();

    let mailto = Mailto {
        to: vec!["kim@proton".into()],
        cc: vec!["mike@proton".into()],
        bcc: vec!["chuck@proton".into()],
        subject: Some("there you go".into()),
        body: Some("kick a man when he's down!".into()),
    };

    let draft = Draft::mailto(&user_ctx, options, mailto).await.unwrap();

    assert_eq!(
        vec![Recipient::Single(SingleRecipient {
            display_name: None,
            email: "kim@proton".into(),
            state: ValidationState::InvalidEmail,
        })],
        draft.recipients(RecipientGroupId::To).await.unwrap(),
    );

    assert_eq!(
        vec![Recipient::Single(SingleRecipient {
            display_name: None,
            email: "mike@proton".into(),
            state: ValidationState::InvalidEmail,
        })],
        draft.recipients(RecipientGroupId::Cc).await.unwrap(),
    );

    assert_eq!(
        vec![Recipient::Single(SingleRecipient {
            display_name: None,
            email: "chuck@proton".into(),
            state: ValidationState::InvalidEmail,
        })],
        draft.recipients(RecipientGroupId::Bcc).await.unwrap(),
    );

    assert_eq!("there you go", draft.subject().await.unwrap());
    assert_eq!("kick a man when he's down!", draft.body().await.unwrap());
}
