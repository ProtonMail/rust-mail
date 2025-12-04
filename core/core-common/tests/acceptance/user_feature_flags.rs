use proton_core_api::services::proton::{
    GetLegacyFeaturesResponse, GetUnleashFeaturesResponse, LegacyFeatureFlag,
    LegacyFeatureFlagMetadata, LegacyFeatureFlagVariant, RangedValue, UnleashToggle,
    UnleashToggleVariant, Value,
};
use proton_core_common::actions::user_feature_flags::OverrideFlag;
use proton_core_common::datatypes::UnixTimestamp;
use proton_core_common::models::UserFeatureFlag;
use proton_core_common::test_utils::test_context::TestContext;
use proton_core_common::test_utils::utils::RespondNthTime;
use serde_json::json;
use stash::orm::Model;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn test_user_feature_flags_cold_start_background_fetch() {
    let ctx = TestContext::new().await;

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

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(GetLegacyFeaturesResponse {
                total: 0,
                features: vec![],
            }),
        )
        .expect(1)
        .named("Empty Legacy fetch")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    assert_eq!(feature_flags.get("TestFeatureA").await.unwrap(), None);
    assert_eq!(feature_flags.get("TestFeatureB").await.unwrap(), None);

    feature_flags.refresh().await.unwrap();

    assert_eq!(feature_flags.get("TestFeatureA").await.unwrap(), Some(true));
    assert_eq!(feature_flags.get("TestFeatureB").await.unwrap(), Some(true));
    assert_eq!(feature_flags.get("NonExistentFeature").await.unwrap(), None);
}

#[tokio::test]
async fn test_user_feature_flags_warm_start_immediate_return() {
    let ctx = TestContext::new().await;

    {
        let past = UnixTimestamp::new(12);
        let user_context = ctx.user_context().await;
        let mut tether = user_context.stash().connection().await.unwrap();
        let mut cached_x = UserFeatureFlag::unleash("CachedFeatureX", past);
        let mut cached_y = UserFeatureFlag::unleash("CachedFeatureY", past);

        tether
            .tx(async move |tx| {
                cached_x.save(tx).await?;
                cached_y.save(tx).await
            })
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

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(GetLegacyFeaturesResponse {
                total: 0,
                features: vec![],
            }),
        )
        .named("Empty Legacy fetch")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    assert_eq!(
        feature_flags.get("CachedFeatureX").await.unwrap(),
        Some(true)
    );
    assert_eq!(
        feature_flags.get("CachedFeatureY").await.unwrap(),
        Some(true)
    );
    assert_eq!(feature_flags.get("UpdatedFeatureZ").await.unwrap(), None);
}

#[tokio::test]
async fn test_user_feature_flags_warm_start_background_refresh() {
    let ctx = TestContext::new().await;

    {
        let past = UnixTimestamp::new(10);
        let user_context = ctx.user_context().await;
        let mut tether = user_context.stash().connection().await.unwrap();
        let mut existing_flag = UserFeatureFlag::unleash("ExistingFeature", past);

        tether
            .tx(async move |tx| existing_flag.save(tx).await)
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

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(GetLegacyFeaturesResponse {
                total: 0,
                features: vec![],
            }),
        )
        .named("Empty Legacy fetch")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    assert_eq!(
        feature_flags.get("ExistingFeature").await.unwrap(),
        Some(true)
    );
    assert_eq!(
        feature_flags.get("NewFeatureFromRefresh").await.unwrap(),
        None
    );

    feature_flags.refresh().await.unwrap();

    assert_eq!(
        feature_flags.get("ExistingFeature").await.unwrap(),
        Some(true)
    );
    assert_eq!(
        feature_flags.get("NewFeatureFromRefresh").await.unwrap(),
        Some(true)
    );

    {
        let user_context = ctx.user_context().await;
        let tether = user_context.stash().connection().await.unwrap();
        let existing_flag = UserFeatureFlag::by_name("ExistingFeature", &tether)
            .await
            .unwrap();
        let new_flag = UserFeatureFlag::by_name("NewFeatureFromRefresh", &tether)
            .await
            .unwrap();

        assert!(existing_flag.unwrap().enabled);
        assert!(new_flag.unwrap().enabled);
    }
}

#[tokio::test]
async fn test_user_feature_flags_network_failure_preserves_cache() {
    let ctx = TestContext::new().await;

    {
        let past = UnixTimestamp::new(5);
        let user_context = ctx.user_context().await;
        let mut tether = user_context.stash().connection().await.unwrap();
        let mut cached_flag = UserFeatureFlag::unleash("CachedFlag", past);

        tether
            .tx(async move |tx| cached_flag.save(tx).await)
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

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "Code": 500,
            "Error": "Internal server error"
        })))
        .named("Legacy network failure")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();
    _ = feature_flags.refresh().await;

    assert_eq!(feature_flags.get("CachedFlag").await.unwrap(), Some(true));
    assert_eq!(feature_flags.get("NonExistentFlag").await.unwrap(), None);
}

#[tokio::test]
async fn test_legacy_feature_flags_basic() {
    let ctx = TestContext::new().await;

    let legacy_response = GetLegacyFeaturesResponse {
        total: 3,
        features: vec![
            test_legacy_boolean_flag("LegacyEnabledFlag", true, false),
            test_legacy_boolean_flag("LegacyDisabledFlag", false, false),
            test_legacy_boolean_flag("LegacyWritableFlag", true, true),
        ],
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GetUnleashFeaturesResponse { toggles: vec![] }),
        )
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(ResponseTemplate::new(200).set_body_json(legacy_response))
        .named("Legacy feature flags")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    feature_flags.refresh().await.unwrap();

    assert_eq!(
        feature_flags.get("LegacyEnabledFlag").await.unwrap(),
        Some(true)
    );
    assert_eq!(
        feature_flags.get("LegacyDisabledFlag").await.unwrap(),
        Some(false)
    );
    assert_eq!(
        feature_flags.get("LegacyWritableFlag").await.unwrap(),
        Some(true)
    );
    assert_eq!(feature_flags.get("NonExistentFlag").await.unwrap(), None);
}

#[tokio::test]
async fn test_legacy_feature_flags_boolean_only_filtering() {
    let ctx = TestContext::new().await;

    let legacy_response = GetLegacyFeaturesResponse {
        total: 5,
        features: vec![
            test_legacy_boolean_flag("BooleanFlag", true, false),
            test_legacy_string_flag("StringFlag", "test_value"),
            test_legacy_integer_flag("IntegerFlag", 42),
            test_legacy_boolean_flag("AnotherBooleanFlag", false, true),
        ],
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GetUnleashFeaturesResponse { toggles: vec![] }),
        )
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(ResponseTemplate::new(200).set_body_json(legacy_response))
        .named("Legacy feature flags with mixed types")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    feature_flags.refresh().await.unwrap();

    assert_eq!(feature_flags.get("BooleanFlag").await.unwrap(), Some(true));
    assert_eq!(
        feature_flags.get("AnotherBooleanFlag").await.unwrap(),
        Some(false)
    );

    assert_eq!(feature_flags.get("StringFlag").await.unwrap(), None);
    assert_eq!(feature_flags.get("IntegerFlag").await.unwrap(), None);
}

#[tokio::test]
async fn test_legacy_feature_flags_writable_property() {
    let ctx = TestContext::new().await;

    let legacy_response = GetLegacyFeaturesResponse {
        total: 2,
        features: vec![
            test_legacy_boolean_flag("WritableFlag", true, true),
            test_legacy_boolean_flag("ReadOnlyFlag", true, false),
        ],
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GetUnleashFeaturesResponse { toggles: vec![] }),
        )
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(ResponseTemplate::new(200).set_body_json(legacy_response))
        .named("Legacy feature flags with writable property")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    feature_flags.refresh().await.unwrap();

    assert_eq!(feature_flags.get("WritableFlag").await.unwrap(), Some(true));
    assert_eq!(feature_flags.get("ReadOnlyFlag").await.unwrap(), Some(true));

    {
        let tether = user_context.stash().connection().await.unwrap();
        let writable_flag = UserFeatureFlag::by_name("WritableFlag", &tether)
            .await
            .unwrap()
            .unwrap();
        let readonly_flag = UserFeatureFlag::by_name("ReadOnlyFlag", &tether)
            .await
            .unwrap()
            .unwrap();

        assert!(writable_flag.writable);
        assert!(!readonly_flag.writable);
    }
}

#[tokio::test]
async fn test_legacy_feature_flags_disappearing_gets_removed() {
    let ctx = TestContext::new().await;

    {
        let past = UnixTimestamp::new(10);
        let user_context = ctx.user_context().await;
        let mut tether = user_context.stash().connection().await.unwrap();
        let mut existing_flag = UserFeatureFlag::legacy("DisappearingFlag", true, true, past);

        tether
            .tx(async move |tx| existing_flag.save(tx).await)
            .await
            .unwrap();
    }

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    let first_response = GetLegacyFeaturesResponse {
        total: 1,
        features: vec![test_legacy_boolean_flag("DisappearingFlag", true, true)],
    };

    let second_response = GetLegacyFeaturesResponse {
        total: 0,
        features: vec![],
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GetUnleashFeaturesResponse { toggles: vec![] }),
        )
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(RespondNthTime::new(
            1,
            ResponseTemplate::new(200).set_body_json(first_response),
            ResponseTemplate::new(200).set_body_json(second_response),
        ))
        .named("Legacy flags - first with flag, then empty")
        .mount(ctx.mock_server())
        .await;

    feature_flags.refresh().await.unwrap();
    assert_eq!(
        feature_flags.get("DisappearingFlag").await.unwrap(),
        Some(true)
    );

    feature_flags.refresh().await.unwrap();

    assert_eq!(feature_flags.get("DisappearingFlag").await.unwrap(), None);

    {
        let tether = user_context.stash().connection().await.unwrap();
        let flag = UserFeatureFlag::by_name("DisappearingFlag", &tether)
            .await
            .unwrap();

        assert!(flag.is_none());
    }
}

#[tokio::test]
async fn test_unleash_vs_legacy_collision_unleash_wins() {
    let ctx = TestContext::new().await;

    let unleash_response = GetUnleashFeaturesResponse {
        toggles: vec![UnleashToggle {
            name: "CollidingFlag".to_string(),
            enabled: true,
            impression_data: false,
            variant: test_unleash_variant(),
        }],
    };

    let legacy_response = GetLegacyFeaturesResponse {
        total: 1,
        features: vec![test_legacy_boolean_flag("CollidingFlag", false, true)],
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(ResponseTemplate::new(200).set_body_json(unleash_response))
        .named("Unleash response")
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(ResponseTemplate::new(200).set_body_json(legacy_response))
        .named("Legacy response")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    feature_flags.refresh().await.unwrap();

    assert_eq!(
        feature_flags.get("CollidingFlag").await.unwrap(),
        Some(true)
    );

    {
        let tether = user_context.stash().connection().await.unwrap();
        let flag = UserFeatureFlag::by_name("CollidingFlag", &tether)
            .await
            .unwrap()
            .unwrap();

        assert!(flag.enabled);
        assert!(!flag.writable);
    }
}

#[tokio::test]
async fn test_legacy_feature_flags_expired_filtering() {
    let ctx = TestContext::new().await;

    let current_time = UnixTimestamp::now();
    let expired_time = current_time.saturating_sub(3600); // 1 hour ago
    let future_time = current_time.saturating_add(3600); // 1 hour in the future

    let legacy_response = GetLegacyFeaturesResponse {
        total: 3,
        features: vec![
            test_legacy_boolean_flag_with_expiration("ExpiredFlag", true, true, expired_time),
            test_legacy_boolean_flag_with_expiration("ValidFlag", true, true, future_time),
            test_legacy_boolean_flag("NonExpiringFlag", false, false), // Default: no expiration
        ],
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GetUnleashFeaturesResponse { toggles: vec![] }),
        )
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(ResponseTemplate::new(200).set_body_json(legacy_response))
        .named("Legacy flags with expiration times")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    feature_flags.refresh().await.unwrap();

    assert_eq!(feature_flags.get("ExpiredFlag").await.unwrap(), None);
    assert_eq!(feature_flags.get("ValidFlag").await.unwrap(), Some(true));
    assert_eq!(
        feature_flags.get("NonExpiringFlag").await.unwrap(),
        Some(false)
    );

    {
        let tether = user_context.stash().connection().await.unwrap();
        let expired_flag = UserFeatureFlag::by_name("ExpiredFlag", &tether)
            .await
            .unwrap();
        let valid_flag = UserFeatureFlag::by_name("ValidFlag", &tether)
            .await
            .unwrap()
            .unwrap();

        assert!(expired_flag.is_none());
        assert!(valid_flag.enabled);
    }
}

#[tokio::test]
async fn test_legacy_feature_flag_becomes_expired_disabled() {
    let ctx = TestContext::new().await;

    {
        let past = UnixTimestamp::new(10);
        let user_context = ctx.user_context().await;
        let mut tether = user_context.stash().connection().await.unwrap();
        let mut existing_flag = UserFeatureFlag::legacy("BecomingExpiredFlag", true, true, past);

        tether
            .tx(async move |tx| existing_flag.save(tx).await)
            .await
            .unwrap();
    }

    let current_time = UnixTimestamp::now();
    let expired_time = current_time.saturating_sub(3600); // 1 hour ago

    let first_response = GetLegacyFeaturesResponse {
        total: 1,
        features: vec![test_legacy_boolean_flag_with_expiration(
            "BecomingExpiredFlag",
            true,
            true,
            expired_time,
        )],
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GetUnleashFeaturesResponse { toggles: vec![] }),
        )
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(ResponseTemplate::new(200).set_body_json(first_response))
        .named("Legacy flag that became expired")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    assert_eq!(
        feature_flags.get("BecomingExpiredFlag").await.unwrap(),
        Some(true)
    );

    feature_flags.refresh().await.unwrap();

    assert_eq!(
        feature_flags.get("BecomingExpiredFlag").await.unwrap(),
        None
    );

    {
        let tether = user_context.stash().connection().await.unwrap();
        let flag = UserFeatureFlag::by_name("BecomingExpiredFlag", &tether)
            .await
            .unwrap();

        assert!(flag.is_none());
    }
}

#[tokio::test]
async fn test_mixed_unleash_and_legacy_sources() {
    let ctx = TestContext::new().await;

    let unleash_response = GetUnleashFeaturesResponse {
        toggles: vec![
            UnleashToggle {
                name: "UnleashOnlyFlag".to_string(),
                enabled: true,
                impression_data: false,
                variant: test_unleash_variant(),
            },
            UnleashToggle {
                name: "UnleashFeatureA".to_string(),
                enabled: true,
                impression_data: false,
                variant: test_unleash_variant(),
            },
        ],
    };

    let legacy_response = GetLegacyFeaturesResponse {
        total: 2,
        features: vec![
            test_legacy_boolean_flag("LegacyOnlyFlag", true, true),
            test_legacy_boolean_flag("LegacyFeatureB", false, false),
        ],
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(ResponseTemplate::new(200).set_body_json(unleash_response))
        .named("Mixed sources - Unleash")
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(ResponseTemplate::new(200).set_body_json(legacy_response))
        .named("Mixed sources - Legacy")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    feature_flags.refresh().await.unwrap();

    assert_eq!(
        feature_flags.get("UnleashOnlyFlag").await.unwrap(),
        Some(true)
    );
    assert_eq!(
        feature_flags.get("UnleashFeatureA").await.unwrap(),
        Some(true)
    );
    assert_eq!(
        feature_flags.get("LegacyOnlyFlag").await.unwrap(),
        Some(true)
    );
    assert_eq!(
        feature_flags.get("LegacyFeatureB").await.unwrap(),
        Some(false)
    );

    {
        let tether = user_context.stash().connection().await.unwrap();
        let unleash_flag = UserFeatureFlag::by_name("UnleashOnlyFlag", &tether)
            .await
            .unwrap()
            .unwrap();
        let legacy_flag = UserFeatureFlag::by_name("LegacyOnlyFlag", &tether)
            .await
            .unwrap()
            .unwrap();

        assert!(!unleash_flag.writable);
        assert!(legacy_flag.writable);
    }
}

#[tokio::test]
async fn test_legacy_feature_flags_pagination() {
    let ctx = TestContext::new().await;

    let mut features = Vec::new();
    for i in 0..155 {
        features.push(test_legacy_boolean_flag(
            &format!("LegacyFlag{i}"),
            i % 2 == 0,
            i % 3 == 0,
        ));
    }

    let first_page = GetLegacyFeaturesResponse {
        total: 155,
        features: features[0..150].to_vec(),
    };

    let second_page = GetLegacyFeaturesResponse {
        total: 155,
        features: features[150..155].to_vec(),
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GetUnleashFeaturesResponse { toggles: vec![] }),
        )
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .and(wiremock::matchers::query_param("Page", "0"))
        .and(wiremock::matchers::query_param("PageSize", "150"))
        .and(wiremock::matchers::query_param("Type", "boolean"))
        .respond_with(ResponseTemplate::new(200).set_body_json(first_page))
        .expect(1)
        .named("Legacy flags - page 1")
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .and(wiremock::matchers::query_param("Page", "1"))
        .and(wiremock::matchers::query_param("PageSize", "150"))
        .and(wiremock::matchers::query_param("Type", "boolean"))
        .respond_with(ResponseTemplate::new(200).set_body_json(second_page))
        .expect(1)
        .named("Legacy flags - page 2")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    feature_flags.refresh().await.unwrap();

    assert_eq!(feature_flags.get("LegacyFlag0").await.unwrap(), Some(true));
    assert_eq!(feature_flags.get("LegacyFlag1").await.unwrap(), Some(false));
    assert_eq!(
        feature_flags.get("LegacyFlag99").await.unwrap(),
        Some(false)
    );
    assert_eq!(
        feature_flags.get("LegacyFlag100").await.unwrap(),
        Some(true)
    );
    assert_eq!(
        feature_flags.get("LegacyFlag104").await.unwrap(),
        Some(true)
    );

    let all_flags = feature_flags.list_all().await;
    assert_eq!(all_flags.len(), 155);
}

#[tokio::test]
async fn test_user_feature_flags_handle_network_failure() {
    let ctx = TestContext::new().await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/tests/ping"))
        .respond_with(ResponseTemplate::new(200))
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(RespondNthTime::new(
            2,
            ResponseTemplate::new(500).set_body_json(json!({
                "Code": 500,
                "Error": "Internal server error"
            })),
            ResponseTemplate::new(200).set_body_json(GetUnleashFeaturesResponse {
                toggles: vec![UnleashToggle {
                    name: "TestFeatureRetry".to_string(),
                    enabled: true,
                    impression_data: false,
                    variant: test_unleash_variant(),
                }],
            }),
        ))
        .expect(3)
        .named("Cold start network failure then success")
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(RespondNthTime::new(
            2,
            ResponseTemplate::new(500).set_body_json(json!({
                "Code": 500,
                "Error": "Internal server error"
            })),
            ResponseTemplate::new(200).set_body_json(GetLegacyFeaturesResponse {
                total: 0,
                features: vec![],
            }),
        ))
        .expect(3)
        .named("Legacy cold start network failure then success")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();
    _ = feature_flags.refresh().await;

    assert_eq!(
        feature_flags.get("TestFeatureRetry").await.unwrap(),
        Some(true)
    );
    assert_eq!(feature_flags.get("NonExistentFeature").await.unwrap(), None);
}

fn test_unleash_variant() -> UnleashToggleVariant {
    UnleashToggleVariant {
        name: "enabled".to_string(),
        feature_enabled: true,
        payload: None,
    }
}

fn test_legacy_boolean_flag(code: &str, enabled: bool, writable: bool) -> LegacyFeatureFlag {
    let year_2100 = UnixTimestamp::new(4_102_444_800);
    test_legacy_boolean_flag_with_expiration(code, enabled, writable, year_2100)
}

fn test_legacy_boolean_flag_with_expiration(
    code: &str,
    enabled: bool,
    writable: bool,
    expiration_time: UnixTimestamp,
) -> LegacyFeatureFlag {
    LegacyFeatureFlag {
        metadata: LegacyFeatureFlagMetadata {
            code: code.to_string(),
            global: false,
            writable,
            expiration_time: Some(expiration_time.as_u64()),
            update_time: None,
        },
        variant: LegacyFeatureFlagVariant::Boolean(Value {
            value: enabled,
            default_value: false,
        }),
    }
}

fn test_legacy_string_flag(code: &str, value: &str) -> LegacyFeatureFlag {
    LegacyFeatureFlag {
        metadata: LegacyFeatureFlagMetadata {
            code: code.to_string(),
            global: false,
            writable: false,
            expiration_time: None,
            update_time: None,
        },
        variant: LegacyFeatureFlagVariant::String(Value {
            value: value.to_string(),
            default_value: "default".to_string(),
        }),
    }
}

fn test_legacy_integer_flag(code: &str, value: i32) -> LegacyFeatureFlag {
    LegacyFeatureFlag {
        metadata: LegacyFeatureFlagMetadata {
            code: code.to_string(),
            global: false,
            writable: false,
            expiration_time: None,
            update_time: None,
        },
        variant: LegacyFeatureFlagVariant::Integer(RangedValue {
            value,
            default_value: 0,
            minimum: 0,
            maximum: 100,
        }),
    }
}

#[tokio::test]
async fn test_override_writable_legacy_flag_success() {
    let ctx = TestContext::new().await;

    let legacy_response = GetLegacyFeaturesResponse {
        total: 1,
        features: vec![test_legacy_boolean_flag("WritableFlag", false, true)],
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GetUnleashFeaturesResponse { toggles: vec![] }),
        )
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(ResponseTemplate::new(200).set_body_json(legacy_response))
        .named("Legacy writable flag")
        .mount(ctx.mock_server())
        .await;

    let override_time = UnixTimestamp::now();
    let put_response = proton_core_api::services::proton::PutFeatureFlagOverrideResponse {
        feature: LegacyFeatureFlag {
            metadata: LegacyFeatureFlagMetadata {
                code: "WritableFlag".to_string(),
                global: false,
                writable: true,
                expiration_time: None,
                update_time: Some(override_time.as_u64()),
            },
            variant: LegacyFeatureFlagVariant::Boolean(Value {
                value: true,
                default_value: false,
            }),
        },
    };

    Mock::given(method("PUT"))
        .and(path("/api/core/v4/features/WritableFlag/value"))
        .respond_with(ResponseTemplate::new(200).set_body_json(put_response))
        .expect(1)
        .named("Override flag API call")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    feature_flags.refresh().await.unwrap();

    assert_eq!(
        feature_flags.get("WritableFlag").await.unwrap(),
        Some(false)
    );

    let action = OverrideFlag::new("WritableFlag".to_string(), true);

    user_context.queue().queue_action(action).await.unwrap();

    let executed_count = user_context
        .queue()
        .new_executor()
        .execute_all()
        .await
        .unwrap();
    assert_eq!(executed_count, 1);

    {
        let tether = user_context.stash().connection().await.unwrap();
        let flag = UserFeatureFlag::by_name("WritableFlag", &tether)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(flag.overridden_to, Some(true));
        assert!(flag.enabled);
        assert!(flag.is_enabled());
    }
}

#[tokio::test]
async fn test_override_non_writable_flag_fails() {
    let ctx = TestContext::new().await;

    let legacy_response = GetLegacyFeaturesResponse {
        total: 1,
        features: vec![test_legacy_boolean_flag("ReadOnlyFlag", true, false)],
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GetUnleashFeaturesResponse { toggles: vec![] }),
        )
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(ResponseTemplate::new(200).set_body_json(legacy_response))
        .named("Legacy read-only flag")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    feature_flags.refresh().await.unwrap();

    assert_eq!(feature_flags.get("ReadOnlyFlag").await.unwrap(), Some(true));

    let action = OverrideFlag::new("ReadOnlyFlag".to_string(), false);

    let result = user_context.queue().queue_action(action).await;

    assert!(result.is_err());
    {
        let tether = user_context.stash().connection().await.unwrap();
        let flag = UserFeatureFlag::by_name("ReadOnlyFlag", &tether)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(flag.overridden_to, None);
        assert!(flag.enabled);
    }
}

#[tokio::test]
async fn test_override_non_existent_flag_fails() {
    let ctx = TestContext::new().await;

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GetUnleashFeaturesResponse { toggles: vec![] }),
        )
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(GetLegacyFeaturesResponse {
                total: 0,
                features: vec![],
            }),
        )
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    feature_flags.refresh().await.unwrap();

    let action = OverrideFlag::new("NonExistentFlag".to_string(), true);

    let result = user_context.queue().queue_action(action).await;

    assert!(result.is_err());
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn test_override_flag_state_preservation() {
    let ctx = TestContext::new().await;

    let legacy_response = GetLegacyFeaturesResponse {
        total: 1,
        features: vec![test_legacy_boolean_flag("StateTestFlag", false, true)],
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GetUnleashFeaturesResponse { toggles: vec![] }),
        )
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(ResponseTemplate::new(200).set_body_json(legacy_response))
        .named("Legacy state test flag")
        .mount(ctx.mock_server())
        .await;

    let override_time_1 = UnixTimestamp::now();
    let put_response_1 = proton_core_api::services::proton::PutFeatureFlagOverrideResponse {
        feature: LegacyFeatureFlag {
            metadata: LegacyFeatureFlagMetadata {
                code: "StateTestFlag".to_string(),
                global: false,
                writable: true,
                expiration_time: None,
                update_time: Some(override_time_1.as_u64()),
            },
            variant: LegacyFeatureFlagVariant::Boolean(Value {
                value: true,
                default_value: false,
            }),
        },
    };

    let override_time_2 = override_time_1.saturating_add(10);
    let put_response_2 = proton_core_api::services::proton::PutFeatureFlagOverrideResponse {
        feature: LegacyFeatureFlag {
            metadata: LegacyFeatureFlagMetadata {
                code: "StateTestFlag".to_string(),
                global: false,
                writable: true,
                expiration_time: None,
                update_time: Some(override_time_2.as_u64()),
            },
            variant: LegacyFeatureFlagVariant::Boolean(Value {
                value: false,
                default_value: false,
            }),
        },
    };

    Mock::given(method("PUT"))
        .and(path("/api/core/v4/features/StateTestFlag/value"))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"Value": true}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(put_response_1))
        .expect(1)
        .named("Override flag API call - first")
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("PUT"))
        .and(path("/api/core/v4/features/StateTestFlag/value"))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"Value": false}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(put_response_2))
        .expect(1)
        .named("Override flag API call - second")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    feature_flags.refresh().await.unwrap();

    // First override: None -> Some(true)
    let action1 = OverrideFlag::new("StateTestFlag".to_string(), true);
    user_context.queue().queue_action(action1).await.unwrap();

    let executed_count = user_context
        .queue()
        .new_executor()
        .execute_all()
        .await
        .unwrap();
    assert_eq!(executed_count, 1);

    {
        let tether = user_context.stash().connection().await.unwrap();
        let flag = UserFeatureFlag::by_name("StateTestFlag", &tether)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(flag.overridden_to, Some(true));
    }

    // Second override: Some(true) -> Some(false)
    let action2 = OverrideFlag::new("StateTestFlag".to_string(), false);
    user_context.queue().queue_action(action2).await.unwrap();

    let executed_count = user_context
        .queue()
        .new_executor()
        .execute_all()
        .await
        .unwrap();
    assert_eq!(executed_count, 1);

    {
        let tether = user_context.stash().connection().await.unwrap();
        let flag = UserFeatureFlag::by_name("StateTestFlag", &tether)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(flag.overridden_to, Some(false));
    }
}

#[tokio::test]
async fn test_override_flag_api_failure_rollback() {
    let ctx = TestContext::new().await;

    let legacy_response = GetLegacyFeaturesResponse {
        total: 1,
        features: vec![test_legacy_boolean_flag("APIFailFlag", false, true)],
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GetUnleashFeaturesResponse { toggles: vec![] }),
        )
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(ResponseTemplate::new(200).set_body_json(legacy_response))
        .named("Legacy API fail flag")
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("PUT"))
        .and(path("/api/core/v4/features/APIFailFlag/value"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "Code": 500,
            "Error": "Internal server error"
        })))
        .named("Failed override API call")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    feature_flags.refresh().await.unwrap();

    let action = OverrideFlag::new("APIFailFlag".to_string(), true);

    user_context.queue().queue_action(action).await.unwrap();

    let _result = user_context.queue().new_executor().execute_all().await;

    {
        let tether = user_context.stash().connection().await.unwrap();
        let flag = UserFeatureFlag::by_name("APIFailFlag", &tether)
            .await
            .unwrap()
            .unwrap();

        assert!(!flag.enabled);
        assert!(!flag.is_enabled());
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn test_override_local_only_not_yet_executed_remotely() {
    let ctx = TestContext::new().await;

    let initial_legacy_response = GetLegacyFeaturesResponse {
        total: 1,
        features: vec![test_legacy_boolean_flag("LocalOnlyFlag", false, true)],
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GetUnleashFeaturesResponse { toggles: vec![] }),
        )
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(ResponseTemplate::new(200).set_body_json(initial_legacy_response.clone()))
        .named("Initial flag state: disabled")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    feature_flags.refresh().await.unwrap();

    assert_eq!(
        feature_flags.get("LocalOnlyFlag").await.unwrap(),
        Some(false)
    );

    let action = OverrideFlag::new("LocalOnlyFlag".to_string(), true);
    user_context.queue().queue_action(action).await.unwrap();

    {
        let tether = user_context.stash().connection().await.unwrap();
        let flag = UserFeatureFlag::by_name("LocalOnlyFlag", &tether)
            .await
            .unwrap()
            .unwrap();

        assert!(!flag.enabled);
        assert_eq!(flag.overridden_to, Some(true));
        assert_eq!(flag.overridden_at, None);
        assert!(flag.is_enabled());
    }

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(ResponseTemplate::new(200).set_body_json(initial_legacy_response))
        .named("Refresh returns same old data")
        .mount(ctx.mock_server())
        .await;

    feature_flags.refresh().await.unwrap();

    {
        let tether = user_context.stash().connection().await.unwrap();
        let flag = UserFeatureFlag::by_name("LocalOnlyFlag", &tether)
            .await
            .unwrap()
            .unwrap();

        assert!(!flag.enabled);
        assert_eq!(flag.overridden_to, Some(true));
        assert_eq!(flag.overridden_at, None);
        assert!(flag.is_enabled());
    }

    assert_eq!(
        feature_flags.get("LocalOnlyFlag").await.unwrap(),
        Some(true)
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn test_backend_returns_stale_data_after_override() {
    let ctx = TestContext::new().await;

    let override_time = UnixTimestamp::now();
    let initial_legacy_response = GetLegacyFeaturesResponse {
        total: 1,
        features: vec![test_legacy_boolean_flag("StaleDataFlag", false, true)],
    };
    let stale_time = override_time.saturating_sub(100);
    let stale_response = GetLegacyFeaturesResponse {
        total: 1,
        features: vec![LegacyFeatureFlag {
            metadata: LegacyFeatureFlagMetadata {
                code: "StaleDataFlag".to_string(),
                global: false,
                writable: true,
                expiration_time: None,
                update_time: Some(stale_time.as_u64()),
            },
            variant: LegacyFeatureFlagVariant::Boolean(Value {
                value: false,
                default_value: false,
            }),
        }],
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GetUnleashFeaturesResponse { toggles: vec![] }),
        )
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(RespondNthTime::new(
            1,
            ResponseTemplate::new(200).set_body_json(initial_legacy_response),
            ResponseTemplate::new(200).set_body_json(stale_response),
        ))
        .named("Initial flag state: disabled")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    // First refresh, using initial response.
    feature_flags.refresh().await.unwrap();

    assert_eq!(
        feature_flags.get("StaleDataFlag").await.unwrap(),
        Some(false)
    );

    let put_response = proton_core_api::services::proton::PutFeatureFlagOverrideResponse {
        feature: LegacyFeatureFlag {
            metadata: LegacyFeatureFlagMetadata {
                code: "StaleDataFlag".to_string(),
                global: false,
                writable: true,
                expiration_time: None,
                update_time: Some(override_time.as_u64()),
            },
            variant: LegacyFeatureFlagVariant::Boolean(Value {
                value: true,
                default_value: false,
            }),
        },
    };

    Mock::given(method("PUT"))
        .and(path("/api/core/v4/features/StaleDataFlag/value"))
        .respond_with(ResponseTemplate::new(200).set_body_json(put_response))
        .expect(1)
        .named("User overrides flag to true")
        .mount(ctx.mock_server())
        .await;

    let action = OverrideFlag::new("StaleDataFlag".to_string(), true);
    user_context.queue().queue_action(action).await.unwrap();

    let executed_count = user_context
        .queue()
        .new_executor()
        .execute_all()
        .await
        .unwrap();
    assert_eq!(executed_count, 1);

    {
        let tether = user_context.stash().connection().await.unwrap();
        let flag = UserFeatureFlag::by_name("StaleDataFlag", &tether)
            .await
            .unwrap()
            .unwrap();

        tracing::warn!("Flag fetched from DB before stale refresh: {:?}", flag);
        assert!(flag.enabled);
        assert_eq!(flag.overridden_to, Some(true));
        assert_eq!(flag.overridden_at, Some(override_time));
        assert!(flag.is_enabled());
    }

    // Second refresh, triggering stale response
    feature_flags.refresh().await.unwrap();

    {
        let tether = user_context.stash().connection().await.unwrap();
        let flag = UserFeatureFlag::by_name("StaleDataFlag", &tether)
            .await
            .unwrap()
            .unwrap();

        tracing::warn!("Flag fetched from DB: {:?}", flag);
        assert!(flag.enabled);
        assert_eq!(flag.overridden_to, Some(true));
        assert_eq!(flag.overridden_at, Some(override_time));
        assert!(flag.is_enabled());
    }

    assert_eq!(
        feature_flags.get("StaleDataFlag").await.unwrap(),
        Some(true)
    );
}

#[tokio::test]
async fn test_override_flag_proper_api_request_format() {
    let ctx = TestContext::new().await;

    let legacy_response = GetLegacyFeaturesResponse {
        total: 1,
        features: vec![test_legacy_boolean_flag("FormatTestFlag", false, true)],
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GetUnleashFeaturesResponse { toggles: vec![] }),
        )
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(ResponseTemplate::new(200).set_body_json(legacy_response))
        .named("Legacy format test flag")
        .mount(ctx.mock_server())
        .await;

    let override_time = UnixTimestamp::now();
    let put_response = proton_core_api::services::proton::PutFeatureFlagOverrideResponse {
        feature: LegacyFeatureFlag {
            metadata: LegacyFeatureFlagMetadata {
                code: "FormatTestFlag".to_string(),
                global: false,
                writable: true,
                expiration_time: None,
                update_time: Some(override_time.as_u64()),
            },
            variant: LegacyFeatureFlagVariant::Boolean(Value {
                value: true,
                default_value: false,
            }),
        },
    };

    Mock::given(method("PUT"))
        .and(path("/api/core/v4/features/FormatTestFlag/value"))
        .and(wiremock::matchers::header(
            "Content-Type",
            "application/json",
        ))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"Value": true}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(put_response))
        .expect(1)
        .named("Override flag with correct format")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    feature_flags.refresh().await.unwrap();

    let action = OverrideFlag::new("FormatTestFlag".to_string(), true);

    user_context.queue().queue_action(action).await.unwrap();

    let executed_count = user_context
        .queue()
        .new_executor()
        .execute_all()
        .await
        .unwrap();
    assert_eq!(executed_count, 1);
}

#[tokio::test]
async fn test_override_flag_api_failure_preserves_existing_override() {
    let ctx = TestContext::new().await;

    let legacy_response = GetLegacyFeaturesResponse {
        total: 1,
        features: vec![test_legacy_boolean_flag("ExistingOverrideFlag", true, true)],
    };

    {
        let user_context = ctx.user_context().await;
        let mut tether = user_context.stash().connection().await.unwrap();
        let mut existing_flag =
            UserFeatureFlag::legacy("ExistingOverrideFlag", true, true, UnixTimestamp::new(10));
        existing_flag.overridden_to = Some(false);

        tether
            .tx(async move |tx| existing_flag.save(tx).await)
            .await
            .unwrap();
    }

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GetUnleashFeaturesResponse { toggles: vec![] }),
        )
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(ResponseTemplate::new(200).set_body_json(legacy_response))
        .named("Legacy flag with existing override")
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("PUT"))
        .and(path("/api/core/v4/features/ExistingOverrideFlag/value"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "Code": 500,
            "Error": "Internal server error"
        })))
        .named("Failed override API call")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    feature_flags.refresh().await.unwrap();

    let action = OverrideFlag::new("ExistingOverrideFlag".to_string(), true);

    user_context.queue().queue_action(action).await.unwrap();

    let _result = user_context.queue().new_executor().execute_all().await;

    {
        let tether = user_context.stash().connection().await.unwrap();
        let flag = UserFeatureFlag::by_name("ExistingOverrideFlag", &tether)
            .await
            .unwrap()
            .unwrap();

        assert!(flag.enabled);
        assert_eq!(flag.overridden_to, Some(false));
        assert!(!flag.is_enabled());
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn test_proton_can_override_user_overridden_flag() {
    let ctx = TestContext::new().await;

    let initial_legacy_response = GetLegacyFeaturesResponse {
        total: 1,
        features: vec![test_legacy_boolean_flag("RatingBoosterFlag", false, true)],
    };

    Mock::given(method("GET"))
        .and(path("/api/feature/v2/frontend"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(GetUnleashFeaturesResponse { toggles: vec![] }),
        )
        .mount(ctx.mock_server())
        .await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(ResponseTemplate::new(200).set_body_json(initial_legacy_response))
        .named("Initial flag state: disabled")
        .mount(ctx.mock_server())
        .await;

    let user_context = ctx.user_context().await;
    let feature_flags = user_context.feature_flags();

    feature_flags.refresh().await.unwrap();

    assert_eq!(
        feature_flags.get("RatingBoosterFlag").await.unwrap(),
        Some(false)
    );

    let override_time = UnixTimestamp::now();
    let put_response = proton_core_api::services::proton::PutFeatureFlagOverrideResponse {
        feature: LegacyFeatureFlag {
            metadata: LegacyFeatureFlagMetadata {
                code: "RatingBoosterFlag".to_string(),
                global: false,
                writable: true,
                expiration_time: None,
                update_time: Some(override_time.as_u64()),
            },
            variant: LegacyFeatureFlagVariant::Boolean(Value {
                value: true,
                default_value: false,
            }),
        },
    };

    Mock::given(method("PUT"))
        .and(path("/api/core/v4/features/RatingBoosterFlag/value"))
        .respond_with(ResponseTemplate::new(200).set_body_json(put_response))
        .expect(1)
        .named("User overrides flag to true")
        .mount(ctx.mock_server())
        .await;

    let action = OverrideFlag::new("RatingBoosterFlag".to_string(), true);
    user_context.queue().queue_action(action).await.unwrap();

    let executed_count = user_context
        .queue()
        .new_executor()
        .execute_all()
        .await
        .unwrap();
    assert_eq!(executed_count, 1);

    {
        let tether = user_context.stash().connection().await.unwrap();
        let flag = UserFeatureFlag::by_name("RatingBoosterFlag", &tether)
            .await
            .unwrap()
            .unwrap();

        assert!(flag.enabled);
        assert_eq!(flag.overridden_to, Some(true));
        assert_eq!(flag.overridden_at, Some(override_time));
        assert!(flag.is_enabled());
    }

    let proton_override_time = override_time.saturating_add(3600);
    let proton_changes_flag_response = GetLegacyFeaturesResponse {
        total: 1,
        features: vec![LegacyFeatureFlag {
            metadata: LegacyFeatureFlagMetadata {
                code: "RatingBoosterFlag".to_string(),
                global: false,
                writable: true,
                expiration_time: None,
                update_time: Some(proton_override_time.as_u64()),
            },
            variant: LegacyFeatureFlagVariant::Boolean(Value {
                value: false,
                default_value: false,
            }),
        }],
    };

    Mock::given(method("GET"))
        .and(path("/api/core/v4/features"))
        .respond_with(ResponseTemplate::new(200).set_body_json(proton_changes_flag_response))
        .named("Proton changes flag back to false")
        .mount(ctx.mock_server())
        .await;

    feature_flags.refresh().await.unwrap();

    assert_eq!(
        feature_flags.get("RatingBoosterFlag").await.unwrap(),
        Some(false)
    );

    {
        let tether = user_context.stash().connection().await.unwrap();
        let flag = UserFeatureFlag::by_name("RatingBoosterFlag", &tether)
            .await
            .unwrap()
            .unwrap();

        assert!(!flag.enabled);
        assert_eq!(flag.overridden_to, None);
        assert_eq!(flag.overridden_at, None);
        assert!(!flag.is_enabled());
    }
}
