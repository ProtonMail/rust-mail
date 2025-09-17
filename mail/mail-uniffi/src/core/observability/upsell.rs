#![allow(clippy::too_many_arguments)]

use super::common::should_record_telemetry;
use crate::mail::MailUserSession;
use proton_core_api::{
    metric,
    services::observability::{ObservabilityMetric, ObservabilityRecorder},
};
use proton_core_common::datatypes::UnixTimestamp;
use proton_mail_common::MailUserContext;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tracing::error;

#[derive(Debug, Error)]
pub enum ObservabilityError {
    #[error("Invalid account creation time: {0}")]
    InvalidTimestamp(String),
    #[error("Future account creation time: {0}")]
    FutureDate(String),
    #[error("Database error: {0}")]
    Database(String),
}

fn calculate_days_from_timestamps(
    create_time: UnixTimestamp,
    current_time: UnixTimestamp,
) -> Result<DaysSinceAccountCreation, ObservabilityError> {
    let create_dt = create_time.to_date_time_utc().ok_or_else(|| {
        ObservabilityError::InvalidTimestamp(format!("Invalid timestamp: {}", create_time.as_u64()))
    })?;

    let current_dt = current_time
        .to_date_time_utc()
        .ok_or_else(|| ObservabilityError::InvalidTimestamp("Invalid current time".to_string()))?;

    let duration = current_dt.signed_duration_since(create_dt);
    let days = duration.num_days();

    if days < 0 {
        return Err(ObservabilityError::FutureDate(format!(
            "Account creation time {create_dt} is in the future"
        )));
    }

    Ok(match days {
        0 => DaysSinceAccountCreation::Zero,
        1..=3 => DaysSinceAccountCreation::OneThroughThree,
        4..=10 => DaysSinceAccountCreation::FourThroughTen,
        11..=30 => DaysSinceAccountCreation::ElevenThroughThirty,
        31..=60 => DaysSinceAccountCreation::ThirtyOneThroughSixty,
        _ => DaysSinceAccountCreation::MoreThanSixty,
    })
}

#[tracing::instrument(skip_all)]
pub async fn calculate_days_since_account_creation(
    mail_user_context: &MailUserContext,
    current_time: UnixTimestamp,
) -> Result<DaysSinceAccountCreation, ObservabilityError> {
    let user = mail_user_context
        .user()
        .await
        .map_err(|e| ObservabilityError::Database(e.to_string()))?;

    calculate_days_from_timestamps(user.create_time, current_time)
}

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum UpsellEntryPoint {
    AutoDeleteMessages,
    ContactGroups,
    DollarPromo,
    FoldersCreation,
    LabelsCreation,
    MailboxTopBar,
    MailboxTopBarPromo,
    NavbarUpsell,
    MobileSignatureEdit,
    PostOnboarding,
    ScheduleSend,
    Snooze,
}

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
pub enum DaysSinceAccountCreation {
    #[serde(rename = "0")]
    Zero,
    #[serde(rename = "01-03")]
    OneThroughThree,
    #[serde(rename = "04-10")]
    FourThroughTen,
    #[serde(rename = "11-30")]
    ElevenThroughThirty,
    #[serde(rename = "31-60")]
    ThirtyOneThroughSixty,
    #[serde(rename = ">60")]
    MoreThanSixty,
    #[serde(rename = "n/a")]
    NotApplicable,
}

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
pub enum ModalVariant {
    Carousel,
}

metric! {
    #[name = "mail_upsell_button_tapped_total"]
    #[version = 1]
    pub struct UpsellButtonTappedTotal {
        pub upsell_entry_point: UpsellEntryPoint,
        pub plan_before_upgrade: String,
        pub days_since_account_creation: DaysSinceAccountCreation,
        pub modal_variant: ModalVariant,
    }
}

metric! {
    #[name = "mail_drive_spotlight_mailbox_button_tapped_total"]
    #[version = 1]
    pub struct DriveSpotlightMailboxButtonTappedTotal {
        pub plan_before_upgrade: String,
        pub days_since_account_creation: DaysSinceAccountCreation,
    }
}

metric! {
    #[name = "mail_drive_spotlight_cta_button_tapped_total"]
    #[version = 1]
    pub struct DriveSpotlightCtaButtonTappedTotal {
        pub plan_before_upgrade: String,
        pub days_since_account_creation: DaysSinceAccountCreation,
    }
}

metric! {
    #[name = "mail_upgrade_attempt_total"]
    #[version = 1]
    pub struct UpgradeAttemptTotal {
        pub upsell_entry_point: UpsellEntryPoint,
        pub plan_before_upgrade: String,
        pub days_since_account_creation: DaysSinceAccountCreation,
        pub modal_variant: ModalVariant,
        pub selected_plan: String,
        pub selected_cycle: String,
        pub upsell_is_promotional: bool,
    }
}

metric! {
    #[name = "mail_upgrade_error_total"]
    #[version = 1]
    pub struct UpgradeErrorTotal {
        pub upsell_entry_point: UpsellEntryPoint,
        pub plan_before_upgrade: String,
        pub days_since_account_creation: DaysSinceAccountCreation,
        pub modal_variant: ModalVariant,
        pub selected_plan: String,
        pub selected_cycle: String,
        pub upsell_is_promotional: bool,
    }
}

metric! {
    #[name = "mail_upgrade_cancelled_by_user_total"]
    #[version = 1]
    pub struct UpgradeCancelledByUserTotal {
        pub upsell_entry_point: UpsellEntryPoint,
        pub plan_before_upgrade: String,
        pub days_since_account_creation: DaysSinceAccountCreation,
        pub modal_variant: ModalVariant,
        pub selected_plan: String,
        pub selected_cycle: String,
        pub upsell_is_promotional: bool,
    }
}

metric! {
    #[name = "mail_upgrade_success_total"]
    #[version = 1]
    pub struct UpgradeSuccessTotal {
        pub upsell_entry_point: UpsellEntryPoint,
        pub plan_before_upgrade: String,
        pub days_since_account_creation: DaysSinceAccountCreation,
        pub modal_variant: ModalVariant,
        pub selected_plan: String,
        pub selected_cycle: String,
        pub upsell_is_promotional: bool,
    }
}

#[uniffi_export]
#[tracing::instrument(skip_all)]
pub async fn record_upsell_button_tapped(
    user_session: Arc<MailUserSession>,
    upsell_entry_point: UpsellEntryPoint,
    plan_before_upgrade: String,
    modal_variant: ModalVariant,
) {
    let user_context = match user_session.ctx() {
        Ok(ctx) => ctx,
        Err(err) => {
            error!("Failed to get user context: {err:?}");
            return;
        }
    };

    let should_record = should_record_telemetry(&user_context).await;

    let days_since_account_creation =
        match calculate_days_since_account_creation(user_context.as_ref(), UnixTimestamp::now())
            .await
        {
            Ok(days) => days,
            Err(err) => {
                error!("Failed to calculate days since account creation: {err:?}");
                return;
            }
        };

    ObservabilityRecorder::default().record(
        UpsellButtonTappedTotal::new(
            upsell_entry_point,
            plan_before_upgrade,
            days_since_account_creation,
            modal_variant,
        ),
        should_record,
    );
}

#[uniffi_export]
#[tracing::instrument(skip_all)]
pub async fn record_drive_spotlight_mailbox_button_tapped(
    user_session: Arc<MailUserSession>,
    plan_before_upgrade: String,
) {
    let user_context = match user_session.ctx() {
        Ok(ctx) => ctx,
        Err(err) => {
            error!("Failed to get user context: {err:?}");
            return;
        }
    };

    let should_record = should_record_telemetry(&user_context).await;

    let days_since_account_creation =
        match calculate_days_since_account_creation(user_context.as_ref(), UnixTimestamp::now())
            .await
        {
            Ok(days) => days,
            Err(err) => {
                error!("Failed to calculate days since account creation: {err:?}");
                return;
            }
        };

    ObservabilityRecorder::default().record(
        DriveSpotlightMailboxButtonTappedTotal::new(
            plan_before_upgrade,
            days_since_account_creation,
        ),
        should_record,
    );
}

#[uniffi_export]
#[tracing::instrument(skip_all)]
pub async fn record_drive_spotlight_cta_button_tapped(
    user_session: Arc<MailUserSession>,
    plan_before_upgrade: String,
) {
    let user_context = match user_session.ctx() {
        Ok(ctx) => ctx,
        Err(err) => {
            error!("Failed to get user context: {err:?}");
            return;
        }
    };

    let should_record = should_record_telemetry(&user_context).await;

    let days_since_account_creation =
        match calculate_days_since_account_creation(user_context.as_ref(), UnixTimestamp::now())
            .await
        {
            Ok(days) => days,
            Err(err) => {
                error!("Failed to calculate days since account creation: {err:?}");
                return;
            }
        };

    ObservabilityRecorder::default().record(
        DriveSpotlightCtaButtonTappedTotal::new(plan_before_upgrade, days_since_account_creation),
        should_record,
    );
}

#[uniffi_export]
#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all)]
pub async fn record_upgrade_attempt(
    user_session: Arc<MailUserSession>,
    upsell_entry_point: UpsellEntryPoint,
    plan_before_upgrade: String,
    modal_variant: ModalVariant,
    selected_plan: String,
    selected_cycle: String,
    upsell_is_promotional: bool,
) {
    let user_context = match user_session.ctx() {
        Ok(ctx) => ctx,
        Err(err) => {
            error!("Failed to get user context: {err:?}");
            return;
        }
    };

    let should_record = should_record_telemetry(&user_context).await;

    let days_since_account_creation =
        match calculate_days_since_account_creation(user_context.as_ref(), UnixTimestamp::now())
            .await
        {
            Ok(days) => days,
            Err(err) => {
                error!("Failed to calculate days since account creation: {err:?}");
                return;
            }
        };

    ObservabilityRecorder::default().record(
        UpgradeAttemptTotal::new(
            upsell_entry_point,
            plan_before_upgrade,
            days_since_account_creation,
            modal_variant,
            selected_plan,
            selected_cycle,
            upsell_is_promotional,
        ),
        should_record,
    );
}

#[uniffi_export]
#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all)]
pub async fn record_upgrade_error(
    user_session: Arc<MailUserSession>,
    upsell_entry_point: UpsellEntryPoint,
    plan_before_upgrade: String,
    modal_variant: ModalVariant,
    selected_plan: String,
    selected_cycle: String,
    upsell_is_promotional: bool,
) {
    let user_context = match user_session.ctx() {
        Ok(ctx) => ctx,
        Err(err) => {
            error!("Failed to get user context: {err:?}");
            return;
        }
    };

    let should_record = should_record_telemetry(&user_context).await;

    let days_since_account_creation =
        match calculate_days_since_account_creation(user_context.as_ref(), UnixTimestamp::now())
            .await
        {
            Ok(days) => days,
            Err(err) => {
                error!("Failed to calculate days since account creation: {err:?}");
                return;
            }
        };

    ObservabilityRecorder::default().record(
        UpgradeErrorTotal::new(
            upsell_entry_point,
            plan_before_upgrade,
            days_since_account_creation,
            modal_variant,
            selected_plan,
            selected_cycle,
            upsell_is_promotional,
        ),
        should_record,
    );
}

#[uniffi_export]
#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all)]
pub async fn record_upgrade_cancelled_by_user(
    user_session: Arc<MailUserSession>,
    upsell_entry_point: UpsellEntryPoint,
    plan_before_upgrade: String,
    modal_variant: ModalVariant,
    selected_plan: String,
    selected_cycle: String,
    upsell_is_promotional: bool,
) {
    let user_context = match user_session.ctx() {
        Ok(ctx) => ctx,
        Err(err) => {
            error!("Failed to get user context: {err:?}");
            return;
        }
    };

    let should_record = should_record_telemetry(&user_context).await;

    let days_since_account_creation =
        match calculate_days_since_account_creation(user_context.as_ref(), UnixTimestamp::now())
            .await
        {
            Ok(days) => days,
            Err(err) => {
                error!("Failed to calculate days since account creation: {err:?}");
                return;
            }
        };

    ObservabilityRecorder::default().record(
        UpgradeCancelledByUserTotal::new(
            upsell_entry_point,
            plan_before_upgrade,
            days_since_account_creation,
            modal_variant,
            selected_plan,
            selected_cycle,
            upsell_is_promotional,
        ),
        should_record,
    );
}

#[uniffi_export]
#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all)]
pub async fn record_upgrade_success(
    user_session: Arc<MailUserSession>,
    upsell_entry_point: UpsellEntryPoint,
    plan_before_upgrade: String,
    modal_variant: ModalVariant,
    selected_plan: String,
    selected_cycle: String,
    upsell_is_promotional: bool,
) {
    let user_context = match user_session.ctx() {
        Ok(ctx) => ctx,
        Err(err) => {
            error!("Failed to get user context: {err:?}");
            return;
        }
    };

    let should_record = should_record_telemetry(&user_context).await;

    let days_since_account_creation =
        match calculate_days_since_account_creation(user_context.as_ref(), UnixTimestamp::now())
            .await
        {
            Ok(days) => days,
            Err(err) => {
                error!("Failed to calculate days since account creation: {err:?}");
                return;
            }
        };

    ObservabilityRecorder::default().record(
        UpgradeSuccessTotal::new(
            upsell_entry_point,
            plan_before_upgrade,
            days_since_account_creation,
            modal_variant,
            selected_plan,
            selected_cycle,
            upsell_is_promotional,
        ),
        should_record,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::observability::common::test_helper;
    use chrono::{DateTime, Duration, Utc};

    #[test]
    fn test_calculate_days_from_timestamps_valid_cases() {
        let account_created = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let create_time = UnixTimestamp::from(account_created);

        // Test 0 days (same day)
        let same_day = account_created;
        let result =
            calculate_days_from_timestamps(create_time, UnixTimestamp::from(same_day)).unwrap();
        assert!(matches!(result, DaysSinceAccountCreation::Zero));

        // Test 1 day later
        let one_day_later = account_created + Duration::days(1);
        let result =
            calculate_days_from_timestamps(create_time, UnixTimestamp::from(one_day_later))
                .unwrap();
        assert!(matches!(result, DaysSinceAccountCreation::OneThroughThree));

        // Test 3 days (boundary)
        let three_days_later = account_created + Duration::days(3);
        let result =
            calculate_days_from_timestamps(create_time, UnixTimestamp::from(three_days_later))
                .unwrap();
        assert!(matches!(result, DaysSinceAccountCreation::OneThroughThree));

        // Test 4 days (next range)
        let four_days_later = account_created + Duration::days(4);
        let result =
            calculate_days_from_timestamps(create_time, UnixTimestamp::from(four_days_later))
                .unwrap();
        assert!(matches!(result, DaysSinceAccountCreation::FourThroughTen));

        // Test 10 days (boundary)
        let ten_days_later = account_created + Duration::days(10);
        let result =
            calculate_days_from_timestamps(create_time, UnixTimestamp::from(ten_days_later))
                .unwrap();
        assert!(matches!(result, DaysSinceAccountCreation::FourThroughTen));

        // Test 30 days (boundary)
        let thirty_days_later = account_created + Duration::days(30);
        let result =
            calculate_days_from_timestamps(create_time, UnixTimestamp::from(thirty_days_later))
                .unwrap();
        assert!(matches!(
            result,
            DaysSinceAccountCreation::ElevenThroughThirty
        ));

        // Test 60 days (boundary)
        let sixty_days_later = account_created + Duration::days(60);
        let result =
            calculate_days_from_timestamps(create_time, UnixTimestamp::from(sixty_days_later))
                .unwrap();
        assert!(matches!(
            result,
            DaysSinceAccountCreation::ThirtyOneThroughSixty
        ));

        // Test 61 days (next range)
        let sixty_one_days_later = account_created + Duration::days(61);
        let result =
            calculate_days_from_timestamps(create_time, UnixTimestamp::from(sixty_one_days_later))
                .unwrap();
        assert!(matches!(result, DaysSinceAccountCreation::MoreThanSixty));
    }

    #[test]
    fn test_calculate_days_from_timestamps_future_date_error() {
        let current_time = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let future_create_time = current_time + Duration::days(1); // Account "created" in future

        let result = calculate_days_from_timestamps(
            UnixTimestamp::from(future_create_time),
            UnixTimestamp::from(current_time),
        );
        assert!(matches!(result, Err(ObservabilityError::FutureDate(_))));
    }

    #[test]
    fn test_calculate_days_from_timestamps_invalid_timestamp() {
        let valid_time = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let invalid_timestamp = UnixTimestamp::from(u64::MAX); // Should be invalid

        let result =
            calculate_days_from_timestamps(invalid_timestamp, UnixTimestamp::from(valid_time));
        assert!(matches!(
            result,
            Err(ObservabilityError::InvalidTimestamp(_))
        ));

        let result =
            calculate_days_from_timestamps(UnixTimestamp::from(valid_time), invalid_timestamp);
        assert!(matches!(
            result,
            Err(ObservabilityError::InvalidTimestamp(_))
        ));
    }

    #[test]
    fn test_upsell_button_tapped_serialization() {
        let serialized = test_helper::serialize_metric(UpsellButtonTappedTotal::new(
            UpsellEntryPoint::MailboxTopBar,
            "free".to_string(),
            DaysSinceAccountCreation::FourThroughTen,
            ModalVariant::Carousel,
        ));

        assert!(serialized.contains("mail_upsell_button_tapped_total"));
        assert!(serialized.contains("mailbox_top_bar"));
        assert!(serialized.contains("04-10"));
        assert!(serialized.contains("Carousel"));
    }

    #[test]
    fn test_upgrade_attempt_serialization() {
        let serialized = test_helper::serialize_metric(UpgradeAttemptTotal::new(
            UpsellEntryPoint::NavbarUpsell,
            "free".to_string(),
            DaysSinceAccountCreation::Zero,
            ModalVariant::Carousel,
            "plus".to_string(),
            "12".to_string(),
            true,
        ));

        assert!(serialized.contains("mail_upgrade_attempt_total"));
        assert!(serialized.contains("navbar_upsell"));
        assert!(serialized.contains("\"0\""));
        assert!(serialized.contains("Carousel"));
        assert!(serialized.contains("true"));
    }

    #[test]
    fn test_drive_spotlight_serialization() {
        let serialized =
            test_helper::serialize_metric(DriveSpotlightMailboxButtonTappedTotal::new(
                "free".to_string(),
                DaysSinceAccountCreation::MoreThanSixty,
            ));

        assert!(serialized.contains("mail_drive_spotlight_mailbox_button_tapped_total"));
        assert!(serialized.contains("\">60\""));
    }
}
