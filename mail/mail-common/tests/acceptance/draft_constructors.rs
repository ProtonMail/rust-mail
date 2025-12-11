use super::drafts_common::*;

mod mailto {
    use super::*;
    use proton_core_api::services::proton::{LabelId, UserId};
    use proton_mail_common::MailUserContext;
    use proton_mail_common::datatypes::{MimeType, SystemLabelId};
    use proton_mail_common::draft::recipients::{Recipient, SingleRecipient, ValidationState};
    use proton_mail_common::draft::{Draft, DraftActor, DraftActorOptions, RecipientGroupId};
    use proton_mail_common::models::MailSettings;
    use proton_mail_common::test_utils::message_body::{
        TEST_USER_ID, message_body_test_message_simple, message_body_test_user_secret,
    };
    use proton_mail_common::test_utils::test_context::MailTestContext;
    use proton_mailto::Mailto;
    use stash::orm::Model;
    use std::sync::Arc;

    async fn setup(mailto: Mailto) -> (MailTestContext, Arc<MailUserContext>, DraftActor) {
        setup_ex(async |_| (), mailto).await
    }

    async fn setup_ex(
        init: impl AsyncFnOnce(&MailUserContext),
        mailto: Mailto,
    ) -> (MailTestContext, Arc<MailUserContext>, DraftActor) {
        let ctx = MailTestContext::with_user_secret_and_user_id(
            message_body_test_user_secret(),
            UserId::from(TEST_USER_ID),
        )
        .await;

        let params = draft_test_params();
        let mut message = message_body_test_message_simple();

        message.metadata.label_ids.push(LabelId::drafts());
        ctx.setup_user(params.clone()).await;

        let uctx = ctx.mail_user_context().await;

        init(&uctx).await;

        // ---

        let options = DraftActorOptions::default();
        let draft = Draft::mailto(&uctx, options, mailto).await.unwrap();

        (ctx, uctx, draft)
    }

    #[tokio::test]
    async fn smoke() {
        let (_ctx, _uctx, draft) = setup(Mailto {
            to: vec!["kim@proton".into()],
            cc: vec!["mike@proton".into()],
            bcc: vec!["chuck@proton".into()],
            subject: Some("there you go".into()),
            body: Some("kick a man when he's down!".into()),
        })
        .await;

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

        assert_eq!(
            "<span>kick a man when he's down!</span>",
            draft.body().await.unwrap()
        );
    }

    #[tokio::test]
    async fn just_subject() {
        let (_ctx, _uctx, draft) = setup(Mailto {
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: Some("love me some chrząszcz".into()),
            body: None,
        })
        .await;

        for group in RecipientGroupId::all() {
            assert!(draft.recipients(group).await.unwrap().is_empty());
        }

        assert_eq!("love me some chrząszcz", draft.subject().await.unwrap());

        assert_eq!(
            "<br><br><br><div class=\"protonmail_signature_block-user\">Sent from rust rest</div>",
            draft.body().await.unwrap()
        );
    }

    #[tokio::test]
    async fn just_body() {
        let (_ctx, _uctx, draft) = setup(Mailto {
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: None,
            body: Some("love me some chrząszcz".into()),
        })
        .await;

        for group in RecipientGroupId::all() {
            assert!(draft.recipients(group).await.unwrap().is_empty());
        }

        assert_eq!("", draft.subject().await.unwrap());

        assert_eq!(
            "<span>love me some chrząszcz</span>",
            draft.body().await.unwrap()
        );
    }

    #[tokio::test]
    async fn body_with_newlines_for_a_text_draft() {
        let (_ctx, _uctx, draft) = setup_ex(
            async |uctx| {
                uctx.user_stash()
                    .connection()
                    .await
                    .unwrap()
                    .tx(async |bond| {
                        let mut settings = MailSettings::get_or_default(bond).await;

                        settings.draft_mime_type = MimeType::TextPlain;
                        settings.save(bond).await
                    })
                    .await
                    .unwrap();
            },
            Mailto {
                to: vec![],
                cc: vec![],
                bcc: vec![],
                subject: None,
                body: Some(
                    "It's the wrong time\n\
                     For somebody new\n\
                     It's a small crime\n\
                     And I got <no> excuse"
                        .into(),
                ),
            },
        )
        .await;

        assert_eq!(
            "It's the wrong time\n\
             For somebody new\n\
             It's a small crime\n\
             And I got <no> excuse",
            draft.body().await.unwrap()
        );
    }

    #[tokio::test]
    async fn body_with_newlines_for_an_html_draft() {
        let (_ctx, _uctx, draft) = setup(Mailto {
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: None,
            body: Some(
                "It's the wrong time\n\
                 For somebody new\n\
                 It's a small crime\n\
                 And I got <no> excuse"
                    .into(),
            ),
        })
        .await;

        assert_eq!(
            "<span>It's the wrong time</span>\
             <div><span>For somebody new</span></div>\
             <div><span>It's a small crime</span></div>\
             <div><span>And I got &lt;no&gt; excuse</span></div>",
            draft.body().await.unwrap()
        );
    }
}
