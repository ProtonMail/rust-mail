use proton_core_api::services::proton::{
    GetUnleashFeaturesResponse, UnleashToggle, UnleashToggleVariant,
};
use proton_core_common::datatypes::{
    BlackFridayWave, NotificationSettings, UpsellEligibility, UpsellType,
};
use proton_core_common::models::{DelinquentState, ModelExtension, Role};
use proton_core_common::models::{PaidSubscription, User};
use proton_mail_common::MailUserContext;
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::test_context::MailTestContext;
use stash::orm::Model;
use stash::stash::{Bond, StashError};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const FF_BLACK_FRIDAY: &str = "MailBlackFriday2025";
const FF_BLACK_FRIDAY_WAVE2: &str = "MailBlackFriday2025Wave2";

#[tokio::test]
async fn standard_upsell() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    setup_feature_flags(&ctx, TestedFeatureFlags::default()).await;

    let user_ctx = ctx.mail_user_context().await;
    let eligibility = user_ctx.upsell_eligibility().await.unwrap();

    assert_eq!(
        eligibility,
        UpsellEligibility::Eligible(UpsellType::Standard)
    );
}

#[tokio::test]
async fn black_friday_wave1() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    setup_feature_flags(
        &ctx,
        TestedFeatureFlags {
            black_friday_enabled: true,
            black_friday_wave2_enabled: false,
        },
    )
    .await;

    let user_ctx = ctx.mail_user_context().await;

    let user_stash = user_ctx.user_stash();
    let mut tether = user_stash.connection().await.unwrap();
    tether
        .tx(async |tx| save_news(&user_ctx, NotificationSettings::IN_APP_NOTIFICATIONS, tx).await)
        .await
        .unwrap();

    let eligibility = user_ctx.upsell_eligibility().await.unwrap();

    assert_eq!(
        eligibility,
        UpsellEligibility::Eligible(UpsellType::BlackFriday(BlackFridayWave::Wave1))
    );
}

#[tokio::test]
async fn black_friday_wave2() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    setup_feature_flags(
        &ctx,
        TestedFeatureFlags {
            black_friday_enabled: true,
            black_friday_wave2_enabled: true,
        },
    )
    .await;

    let user_ctx = ctx.mail_user_context().await;
    let user_stash = user_ctx.user_stash();
    let mut tether = user_stash.connection().await.unwrap();
    tether
        .tx(async |tx| save_news(&user_ctx, NotificationSettings::IN_APP_NOTIFICATIONS, tx).await)
        .await
        .unwrap();

    let eligibility = user_ctx.upsell_eligibility().await.unwrap();

    assert_eq!(
        eligibility,
        UpsellEligibility::Eligible(UpsellType::BlackFriday(BlackFridayWave::Wave2))
    );
}

#[tokio::test]
async fn black_friday_wave2_but_promo_ended() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    setup_feature_flags(
        &ctx,
        TestedFeatureFlags {
            black_friday_enabled: false,
            black_friday_wave2_enabled: true,
        },
    )
    .await;

    let user_ctx = ctx.mail_user_context().await;
    let eligibility = user_ctx.upsell_eligibility().await.unwrap();

    assert_eq!(
        eligibility,
        UpsellEligibility::Eligible(UpsellType::Standard)
    );
}

#[tokio::test]
async fn paid_user_not_eligible() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    setup_feature_flags(&ctx, TestedFeatureFlags::default()).await;

    let user_ctx = ctx.mail_user_context().await;

    let user_stash = user_ctx.user_stash();
    let mut tether = user_stash.connection().await.unwrap();
    tether
        .tx(async |tx| save_subscription(&user_ctx, PaidSubscription::MAIL, tx).await)
        .await
        .unwrap();

    let eligibility = user_ctx.upsell_eligibility().await.unwrap();
    assert_eq!(eligibility, UpsellEligibility::NotEligible);
}

// We do not show this upsell promotion for users that are not paying for mail but are still
// paying for other services.
#[tokio::test]
async fn paid_user_other_services_not_eligible() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    setup_feature_flags(&ctx, TestedFeatureFlags::default()).await;

    let user_ctx = ctx.mail_user_context().await;

    let user_stash = user_ctx.user_stash();
    let mut tether = user_stash.connection().await.unwrap();
    tether
        .tx(async |tx| save_subscription(&user_ctx, PaidSubscription::VPN, tx).await)
        .await
        .unwrap();

    let eligibility = user_ctx.upsell_eligibility().await.unwrap();
    assert_eq!(eligibility, UpsellEligibility::NotEligible);
}

#[tokio::test]
async fn member_role_not_eligible() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    setup_feature_flags(&ctx, TestedFeatureFlags::default()).await;

    let user_ctx = ctx.mail_user_context().await;

    let user_stash = user_ctx.user_stash();
    let mut tether = user_stash.connection().await.unwrap();
    tether
        .tx(async |tx| save_role(&user_ctx, Role::Member, tx).await)
        .await
        .unwrap();

    let eligibility = user_ctx.upsell_eligibility().await.unwrap();
    assert_eq!(eligibility, UpsellEligibility::NotEligible);
}

#[tokio::test]
async fn black_friday_disabled_notifications() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    setup_feature_flags(
        &ctx,
        TestedFeatureFlags {
            black_friday_enabled: true,
            black_friday_wave2_enabled: false,
        },
    )
    .await;

    let user_ctx = ctx.mail_user_context().await;

    let user_stash = user_ctx.user_stash();
    let mut tether = user_stash.connection().await.unwrap();
    tether
        .tx(async |tx| save_news(&user_ctx, NotificationSettings::ANNOUNCEMENTS, tx).await)
        .await
        .unwrap();

    let eligibility = user_ctx.upsell_eligibility().await.unwrap();
    assert_eq!(
        eligibility,
        UpsellEligibility::Eligible(UpsellType::Standard)
    );
}

#[tokio::test]
async fn black_friday_delinquent_user() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    setup_feature_flags(
        &ctx,
        TestedFeatureFlags {
            black_friday_enabled: true,
            black_friday_wave2_enabled: false,
        },
    )
    .await;

    let user_ctx = ctx.mail_user_context().await;

    let user_stash = user_ctx.user_stash();
    let mut tether = user_stash.connection().await.unwrap();
    tether
        .tx(async |tx| {
            save_delinquency(&user_ctx, DelinquentState::Delinquent, tx).await?;
            save_news(&user_ctx, NotificationSettings::IN_APP_NOTIFICATIONS, tx).await
        })
        .await
        .unwrap();

    let eligibility = user_ctx.upsell_eligibility().await.unwrap();
    assert_eq!(
        eligibility,
        UpsellEligibility::Eligible(UpsellType::Standard)
    );
}

#[derive(Default)]
struct TestedFeatureFlags {
    black_friday_enabled: bool,
    black_friday_wave2_enabled: bool,
}

async fn save_subscription(
    ctx: &MailUserContext,
    subscription: PaidSubscription,
    tx: &Bond<'_>,
) -> Result<(), StashError> {
    let mut user = User::find_by_id(ctx.user_id().clone(), tx).await?.unwrap();
    user.subscribed = subscription;
    user.save(tx).await
}

async fn save_role(ctx: &MailUserContext, role: Role, tx: &Bond<'_>) -> Result<(), StashError> {
    let mut user = User::find_by_id(ctx.user_id().clone(), tx).await?.unwrap();
    user.role = role;
    user.save(tx).await
}

async fn save_delinquency(
    ctx: &MailUserContext,
    delinquency: DelinquentState,
    tx: &Bond<'_>,
) -> Result<(), StashError> {
    let mut user = User::find_by_id(ctx.user_id().clone(), tx).await?.unwrap();
    user.delinquent = delinquency;
    user.save(tx).await
}

async fn save_news(
    ctx: &MailUserContext,
    news: NotificationSettings,
    tx: &Bond<'_>,
) -> Result<(), StashError> {
    let mut settings =
        proton_core_common::models::UserSettings::find_by_id(ctx.user_id().clone(), tx)
            .await?
            .unwrap();
    settings.news = news;
    settings.save(tx).await
}

fn test_unleash_variant() -> UnleashToggleVariant {
    UnleashToggleVariant {
        name: "enabled".to_string(),
        feature_enabled: true,
        payload: None,
    }
}

async fn setup_feature_flags(ctx: &MailTestContext, flags: TestedFeatureFlags) {
    let mut toggles = vec![];

    if flags.black_friday_enabled {
        toggles.push(UnleashToggle {
            name: FF_BLACK_FRIDAY.to_string(),
            enabled: true,
            impression_data: false,
            variant: test_unleash_variant(),
        });
    }

    if flags.black_friday_wave2_enabled {
        toggles.push(UnleashToggle {
            name: FF_BLACK_FRIDAY_WAVE2.to_string(),
            enabled: true,
            impression_data: false,
            variant: test_unleash_variant(),
        });
    }

    let mock_response = GetUnleashFeaturesResponse { toggles };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mock_response))
        .named("Feature flags setup")
        .mount(ctx.mock_server())
        .await;

    ctx.user_context()
        .await
        .feature_flags()
        .refresh()
        .await
        .expect("Fresh feature flags");
}
