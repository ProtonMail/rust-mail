use std::time::Duration;

use proton_core_common::models::AppSettings;
use proton_core_common::services::Service;
use proton_mail_api::services::proton::response_data::{UnleashToggle, UnleashToggleVariant};
use proton_mail_api::services::proton::responses::GetUnleashFeaturesResponse;
use proton_mail_common::feature_flags::FeatureFlagsService;
use proton_mail_common::test_utils::test_context::MailTestContext;
use serde_json::json;
use stash::orm::Model;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

async fn setup_feature_flags_service(ctx: &MailTestContext) -> FeatureFlagsService {
    let core_context = ctx.core_context();
    let weak_ctx = std::sync::Arc::downgrade(core_context);
    let service = FeatureFlagsService::new(weak_ctx);

    service
        .init()
        .await
        .expect("Failed to initialize FeatureFlagsService");
    service
}

fn test_unleash_variant() -> UnleashToggleVariant {
    UnleashToggleVariant {
        name: "enabled".to_string(),
        feature_enabled: true,
        payload: None,
    }
}

#[tokio::test]
async fn test_feature_flags_cold_start_blocks_on_fetch() {
    let ctx = MailTestContext::new().await;

    let mock_response = GetUnleashFeaturesResponse {
        toggles: vec![
            UnleashToggle {
                name: "TestFeatureA".to_string(),
                enabled: true,
                impression_data: false,
                variant: test_unleash_variant(),
            },
            UnleashToggle {
                name: "TestFeatureB".to_string(),
                enabled: true,
                impression_data: false,
                variant: test_unleash_variant(),
            },
        ],
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mock_response))
        .expect(1)
        .named("Cold start Unleash fetch")
        .mount(ctx.mock_server())
        .await;

    let feature_flags = setup_feature_flags_service(&ctx).await;

    assert_eq!(feature_flags.get("TestFeatureA").await, Some(true));
    assert_eq!(feature_flags.get("TestFeatureB").await, Some(true));
    assert_eq!(feature_flags.get("NonExistentFeature").await, None);
}

#[tokio::test]
async fn test_feature_flags_warm_start_immediate_return() {
    color_backtrace::install();

    let ctx = MailTestContext::new().await;

    {
        let mut tether = ctx.core_context().account_stash().connection();
        let mut app_settings = AppSettings::get_or_default(&tether).await;
        app_settings
            .app_features
            .features
            .insert("CachedFeatureX".to_string(), true);
        app_settings
            .app_features
            .features
            .insert("CachedFeatureY".to_string(), true);

        tether
            .tx(async move |tx| app_settings.save(tx).await)
            .await
            .unwrap();
    }

    let updated_response = GetUnleashFeaturesResponse {
        toggles: vec![
            UnleashToggle {
                name: "CachedFeatureX".to_string(),
                enabled: true,
                impression_data: false,
                variant: test_unleash_variant(),
            },
            UnleashToggle {
                name: "UpdatedFeatureZ".to_string(),
                enabled: true,
                impression_data: false,
                variant: test_unleash_variant(),
            },
        ],
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(ResponseTemplate::new(200).set_body_json(updated_response))
        .named("Background refresh")
        .mount(ctx.mock_server())
        .await;

    let feature_flags = setup_feature_flags_service(&ctx).await;

    assert_eq!(feature_flags.get("CachedFeatureX").await, Some(true));
    assert_eq!(feature_flags.get("CachedFeatureY").await, Some(true));
    assert_eq!(feature_flags.get("UpdatedFeatureZ").await, None); // Not yet refreshed
}

#[tokio::test]
async fn test_feature_flags_warm_start_background_refresh() {
    let ctx = MailTestContext::new().await;

    {
        let mut tether = ctx.core_context().account_stash().connection();
        let mut app_settings = AppSettings::get_or_default(&tether).await;
        app_settings
            .app_features
            .features
            .insert("ExistingFeature".to_string(), true);

        tether
            .tx(async move |tx| app_settings.save(tx).await)
            .await
            .unwrap();
    }

    let refresh_response = GetUnleashFeaturesResponse {
        toggles: vec![
            UnleashToggle {
                name: "ExistingFeature".to_string(),
                enabled: true,
                impression_data: false,
                variant: test_unleash_variant(),
            },
            UnleashToggle {
                name: "NewFeatureFromRefresh".to_string(),
                enabled: true,
                impression_data: false,
                variant: test_unleash_variant(),
            },
        ],
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(ResponseTemplate::new(200).set_body_json(refresh_response))
        .named("Background refresh with new flag")
        .mount(ctx.mock_server())
        .await;

    let feature_flags = setup_feature_flags_service(&ctx).await;

    assert_eq!(feature_flags.get("ExistingFeature").await, Some(true));
    assert_eq!(feature_flags.get("NewFeatureFromRefresh").await, None);

    let mut attempts = 4;

    while attempts > 0 {
        if feature_flags.get("NewFeatureFromRefresh").await.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
        attempts -= 1;
    }

    assert_eq!(feature_flags.get("ExistingFeature").await, Some(true));
    assert_eq!(feature_flags.get("NewFeatureFromRefresh").await, Some(true));

    {
        let tether = ctx.core_context().account_stash().connection();
        let app_settings = AppSettings::get_or_default(&tether).await;

        assert_eq!(
            app_settings.app_features.features.get("ExistingFeature"),
            Some(&true)
        );
        assert_eq!(
            app_settings
                .app_features
                .features
                .get("NewFeatureFromRefresh"),
            Some(&true)
        );
    }
}

#[tokio::test]
async fn test_feature_flags_network_failure_preserves_cache() {
    let ctx = MailTestContext::new().await;

    {
        let mut tether = ctx.core_context().account_stash().connection();
        let mut app_settings = AppSettings::get_or_default(&tether).await;
        app_settings
            .app_features
            .features
            .insert("CachedFlag".to_string(), true);

        tether
            .tx(async move |tx| app_settings.save(tx).await)
            .await
            .unwrap();
    }

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "Code": 500,
            "Error": "Internal server error"
        })))
        .named("Network failure")
        .mount(ctx.mock_server())
        .await;

    let feature_flags = setup_feature_flags_service(&ctx).await;

    assert_eq!(feature_flags.get("CachedFlag").await, Some(true));
    assert_eq!(feature_flags.get("NonExistentFlag").await, None);

    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(feature_flags.get("CachedFlag").await, Some(true));
}

#[tokio::test]
async fn test_feature_flags_cold_start_network_failure() {
    let ctx = MailTestContext::new().await;

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "Code": 500,
            "Error": "Internal server error"
        })))
        .expect(4) // HTTP client will retry on 500 errors
        .named("Cold start network failure")
        .mount(ctx.mock_server())
        .await;

    let feature_flags = setup_feature_flags_service(&ctx).await;

    assert_eq!(feature_flags.get("AnyFlag").await, None);
    assert_eq!(feature_flags.get("AnotherFlag").await, None);
}
