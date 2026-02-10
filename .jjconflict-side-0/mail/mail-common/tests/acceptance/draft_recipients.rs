use proton_core_api::consts::CoreBundle;
use proton_core_api::services::proton::GetKeysAllResponse;
use proton_core_api::services::proton::UserId;
use proton_core_api::services::proton::common::ApiErrorInfo;
use proton_crypto_account::keys::EmailMimeType;
use proton_mail_common::draft::recipients::OnBackgroundValidationComplete;
use proton_mail_common::draft::recipients::OnPrivacyLockUpdate;
use proton_mail_common::draft::recipients::PrivacyLockState;
use proton_mail_common::draft::recipients::RecipientPrivacyLockUpdate;
use proton_mail_common::draft::recipients::RecipientValidationUpdate;
use proton_mail_common::draft::recipients::{
    Recipient, RecipientEntry, RecipientList, ValidatingRecipientList, ValidationState,
};
use proton_mail_common::models::DraftMetadata;
use proton_mail_common::models::MetadataId;
use proton_mail_common::test_utils::init::Params;
use proton_mail_common::test_utils::message_body::message_body_test_params;
use proton_mail_common::test_utils::message_body::{TEST_USER_ID, message_body_test_user_secret};
use proton_mail_common::test_utils::test_context::MailTestContext;
use test_case::test_case;
use tokio_util::sync::CancellationToken;

pub struct ChannelBackgroundValidationComplete<T>(flume::Sender<T>);

impl<T> Clone for ChannelBackgroundValidationComplete<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> ChannelBackgroundValidationComplete<T> {
    pub fn new(capacity: usize) -> (Self, flume::Receiver<T>) {
        let (sender, receiver) = flume::bounded(capacity);
        (Self(sender), receiver)
    }
}

impl OnBackgroundValidationComplete
    for ChannelBackgroundValidationComplete<RecipientValidationUpdate>
{
    async fn recipients_validation_state_updated(&self, updates: RecipientValidationUpdate) {
        let _ = self.0.send_async(updates).await;
    }
}

impl OnPrivacyLockUpdate for ChannelBackgroundValidationComplete<RecipientValidationUpdate> {
    async fn recipient_privacy_lock_updated(&self, _: RecipientPrivacyLockUpdate) {
        unreachable!();
    }
}

impl OnBackgroundValidationComplete
    for ChannelBackgroundValidationComplete<RecipientPrivacyLockUpdate>
{
    async fn recipients_validation_state_updated(&self, _: RecipientValidationUpdate) {
        //do nothing
    }
}

impl OnPrivacyLockUpdate for ChannelBackgroundValidationComplete<RecipientPrivacyLockUpdate> {
    async fn recipient_privacy_lock_updated(&self, updates: RecipientPrivacyLockUpdate) {
        let _ = self.0.send_async(updates).await;
    }
}

#[test_case(TEST_EMAIL_1,
    success_response(false),
    ValidationState::Valid{official:false, proton:false}
; "Valid non proton address")]
#[test_case(TEST_EMAIL_3,
    success_response(true),
    ValidationState::Valid{official:true, proton:true}
; "Valid proton address")]
#[test_case(TEST_EMAIL_2,
    failure_invalid_email(),
    ValidationState::InvalidEmail
; "Invalid Email")]
#[test_case(TEST_EMAIL_3,
    failure_proton_address_does_not_exist(),
    ValidationState::DoesNotExist
; "Proton address does not exist")]
#[test_case(TEST_EMAIL_3,
    failure_unknown(),
    ValidationState::Unknown
; "Unknown Error")]
#[tokio::test]
async fn single_recipient_validation(email: &str, response: Response, state: ValidationState) {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let mut recipient_list = RecipientList::new();
    let (cb, receiver) = ChannelBackgroundValidationComplete::<RecipientValidationUpdate>::new(1);
    let cancellation_token = CancellationToken::new();
    let mut list = ValidatingRecipientList::new(
        cancellation_token.clone(),
        cancellation_token,
        &mut recipient_list,
        cb,
        MetadataId(1),
        EmailMimeType::Html,
        false,
    );

    let params = Params::default_basic();
    ctx.setup_user(params).await;
    match response {
        Response::Success(r) => {
            ctx.core_test_context
                .mock_get_keys_all_with_internal_param(email, Some(false), r)
                .await;
        }
        Response::Failure(r) => {
            ctx.core_test_context
                .mock_get_keys_all_failure(email, Some(false), r)
                .await;
        }
    };
    let user_ctx = ctx.mail_user_context().await;

    list.add_single(
        &user_ctx,
        RecipientEntry {
            name: None,
            email: email.into(),
        },
    )
    .unwrap();

    let updates = receiver.recv_async().await.unwrap();

    drop(list);
    updates.apply(&mut recipient_list);
    let recipients = recipient_list.recipients();
    assert_eq!(recipients.len(), 1);
    match &recipients[0] {
        Recipient::Single(r) => {
            assert_eq!(r.email.as_clear_text_str(), email);
            assert_eq!(r.state, state);
        }
        Recipient::Group(_) => {
            panic!("Unexpected group recipient")
        }
    }
}

#[test_case(TEST_EMAIL_1,
    success_response(false),
    ValidationState::Valid{official:false, proton:false}
; "Valid non proton address")]
#[test_case(TEST_EMAIL_3,
    success_response(true),
    ValidationState::Valid{official:true, proton:true}
; "Valid proton address")]
#[test_case(TEST_EMAIL_2,
    failure_invalid_email(),
    ValidationState::InvalidEmail
; "Invalid Email")]
#[test_case(TEST_EMAIL_3,
    failure_proton_address_does_not_exist(),
    ValidationState::DoesNotExist
; "Proton address does not exist")]
#[test_case(TEST_EMAIL_3,
    failure_unknown(),
    ValidationState::Unknown
; "Unknown Error")]
#[tokio::test]
async fn group_recipient_validation(email: &str, response: Response, state: ValidationState) {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let mut recipient_list = RecipientList::new();
    let (cb, receiver) = ChannelBackgroundValidationComplete::<RecipientValidationUpdate>::new(1);
    let cancellation_token = CancellationToken::new();
    let mut list = ValidatingRecipientList::new(
        cancellation_token.clone(),
        cancellation_token,
        &mut recipient_list,
        cb,
        MetadataId(0),
        EmailMimeType::Html,
        false,
    );

    let params = Params::default_basic();
    ctx.setup_user(params).await;
    match response {
        Response::Success(r) => {
            ctx.core_test_context
                .mock_get_keys_all_with_internal_param(email, Some(false), r)
                .await;
        }
        Response::Failure(r) => {
            ctx.core_test_context
                .mock_get_keys_all_failure(email, Some(false), r)
                .await;
        }
    };
    let user_ctx = ctx.mail_user_context().await;

    list.add_group(
        &user_ctx,
        "my_group".to_owned().try_into().unwrap(),
        [RecipientEntry {
            name: None,
            email: email.into(),
        }],
        1,
    );

    let updates = receiver.recv_async().await.unwrap();
    drop(list);

    updates.apply(&mut recipient_list);

    let recipients = recipient_list.recipients();
    assert_eq!(recipients.len(), 1);
    match &recipients[0] {
        Recipient::Group(group) => {
            let r = &group.recipients[0];
            assert_eq!(r.email.as_clear_text_str(), email);
            assert_eq!(r.state, state);
        }
        Recipient::Single(_) => {
            panic!("Unexpected group recipient")
        }
    }
}

#[tokio::test]
async fn lock_calculation() {
    // Check that we invoke all the steps required for lock calculation, even though the result
    // will be None.
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let email = "foo@proton.com";

    let mut recipient_list = RecipientList::new();
    let (cb, receiver) = ChannelBackgroundValidationComplete::<RecipientPrivacyLockUpdate>::new(1);
    let cancellation_token = CancellationToken::new();

    let params = message_body_test_params();
    ctx.setup_user(params).await;
    ctx.core_test_context
        .mock_get_keys_all_with_internal_param(
            email,
            Some(false),
            GetKeysAllResponse {
                address_keys: Default::default(),
                catch_all_keys: None,
                is_proton: false,
                proton_mx: false,
                unverified_keys: None,
                warnings: vec![],
            },
        )
        .await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let draft_metadata = tether
        .tx(async |tx| DraftMetadata::empty(tx).await)
        .await
        .unwrap();

    recipient_list
        .add_single(RecipientEntry {
            name: None,
            email: email.into(),
        })
        .unwrap();

    let mut list = ValidatingRecipientList::new(
        cancellation_token.clone(),
        cancellation_token,
        &mut recipient_list,
        cb,
        draft_metadata.id.unwrap(),
        EmailMimeType::Html,
        false,
    );

    list.recalculate_all_privacy_locks(&user_ctx);

    let updates = receiver.recv_async().await.unwrap();

    drop(list);
    updates.apply(&mut recipient_list);
    let recipients = recipient_list.recipients();
    assert_eq!(recipients.len(), 1);
    match &recipients[0] {
        Recipient::Single(r) => {
            assert!(matches!(r.privacy_lock, PrivacyLockState::Calculated(None)));
        }
        Recipient::Group(_) => {
            panic!("Unexpected group recipient")
        }
    }
}

#[tokio::test]
async fn lock_calculation_byoe() {
    // when byoe is true we go straitgh to the end result, no requests are made
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let email = "foo@proton.com";

    let mut recipient_list = RecipientList::new();
    let (cb, receiver) = ChannelBackgroundValidationComplete::<RecipientPrivacyLockUpdate>::new(1);
    let cancellation_token = CancellationToken::new();

    let params = message_body_test_params();
    ctx.setup_user(params).await;

    let user_ctx = ctx.mail_user_context().await;

    recipient_list
        .add_single(RecipientEntry {
            name: None,
            email: email.into(),
        })
        .unwrap();

    let mut list = ValidatingRecipientList::new(
        cancellation_token.clone(),
        cancellation_token,
        &mut recipient_list,
        cb,
        MetadataId(1),
        EmailMimeType::Html,
        true,
    );

    list.recalculate_all_privacy_locks(&user_ctx);

    let updates = receiver.recv_async().await.unwrap();

    drop(list);
    updates.apply(&mut recipient_list);
    let recipients = recipient_list.recipients();
    assert_eq!(recipients.len(), 1);
    match &recipients[0] {
        Recipient::Single(r) => {
            assert!(matches!(r.privacy_lock, PrivacyLockState::Calculated(None)));
        }
        Recipient::Group(_) => {
            panic!("Unexpected group recipient")
        }
    }
}

const TEST_EMAIL_1: &str = "foo@bar.com";
const TEST_EMAIL_2: &str = "bar@bar.com";
const TEST_EMAIL_3: &str = "bar@proton.me";

#[allow(clippy::large_enum_variant)]
enum Response {
    Success(GetKeysAllResponse),
    Failure(ApiErrorInfo),
}

fn success_response(is_proton: bool) -> Response {
    Response::Success(GetKeysAllResponse {
        address_keys: Default::default(),
        catch_all_keys: None,
        is_proton,
        proton_mx: false,
        unverified_keys: None,
        warnings: vec![],
    })
}

fn failure_invalid_email() -> Response {
    Response::Failure(ApiErrorInfo {
        code: CoreBundle::KeyGetInputInvalid as u32,
        error: None,
        details: None,
    })
}

fn failure_proton_address_does_not_exist() -> Response {
    Response::Failure(ApiErrorInfo {
        code: CoreBundle::KeyGetAddressMissing as u32,
        error: None,
        details: None,
    })
}

fn failure_unknown() -> Response {
    Response::Failure(ApiErrorInfo {
        code: u32::MAX,
        error: None,
        details: None,
    })
}
