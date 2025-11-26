use super::drafts_common::*;
use proton_core_api::services::proton::UserId;
use proton_mail_common::{
    IosShareExtDraft, IosShareExtension,
    draft::Draft,
    test_utils::{
        message_body::{TEST_USER_ID, message_body_test_user_secret},
        test_context::MailTestContext,
    },
};

#[tokio::test]
async fn smoke() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    ctx.setup_user(draft_test_params()).await;

    let user_ctx = ctx.mail_user_context().await;

    // ---

    IosShareExtension::init_draft(ctx.mail_context.mail_cache_path()).unwrap();

    IosShareExtension::save_draft(
        ctx.mail_context.mail_cache_path(),
        IosShareExtDraft {
            subject: Some("quotes.com".into()),
            body: Some("Here's a quote:<br>A quote".into()),
            inline_attachments: vec![],
            attachments: vec![],
        },
    )
    .unwrap();

    let draft = Draft::from_ios_share_extension(&user_ctx, Default::default())
        .await
        .unwrap();

    assert_eq!(
        "Here's a quote:<br>A quote<br><br><div class=\"protonmail_signature_b\
         lock-user\">Sent from rust rest</div>",
        draft.body().await.unwrap()
    );

    assert_eq!("quotes.com", draft.subject().await.unwrap());
}
