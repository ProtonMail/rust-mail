use proton_core_common::{
    metric,
    observability::{ObservabilityMetric, PreLoginMetricRecorder},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "camelCase")]
pub enum PaymentObservabilityMetric {
    IapSubscribe(PaymentObservabilityResponse),
    SendPaymentToken(PaymentObservabilityResponse),
    CreateSubscription(PaymentObservabilityResponse),
    GetSubscription(PaymentObservabilityResponse),
    GetPlans(PaymentObservabilityResponse),
}

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "camelCase")]
pub enum PaymentObservabilityResponse {
    Success,
    Http4xx,
    Http5xx,
    SerializationError,
    Unknown,
}

metric! {
    #[name = "payments_iap_subscribe_total"]
    #[version = 1]
    pub struct IapSubscribeMetric { pub status: PaymentObservabilityResponse }
}
metric! {
    #[name = "payments_iap_send_payment_token_total"]
    #[version = 1]
    pub struct SendPaymentTokenMetric { pub status: PaymentObservabilityResponse }
}
metric! {
    #[name = "payments_iap_create_subscription_total"]
    #[version = 1]
    pub struct CreateSubscriptionMetric { pub status: PaymentObservabilityResponse }
}
metric! {
    #[name = "payments_get_subscription_total"]
    #[version = 1]
    pub struct GetSubscriptionMetric { pub status: PaymentObservabilityResponse }
}
metric! {
    #[name = "payments_get_plans_total"]
    #[version = 1]
    pub struct GetPlansMetric { pub status: PaymentObservabilityResponse }
}

impl PaymentObservabilityMetric {
    pub fn record(self, recorder: &PreLoginMetricRecorder) {
        match self {
            Self::IapSubscribe(response) => {
                recorder.record(IapSubscribeMetric::new(response));
            }
            Self::SendPaymentToken(response) => {
                recorder.record(SendPaymentTokenMetric::new(response));
            }
            Self::CreateSubscription(response) => {
                recorder.record(CreateSubscriptionMetric::new(response));
            }
            Self::GetSubscription(response) => {
                recorder.record(GetSubscriptionMetric::new(response));
            }
            Self::GetPlans(response) => {
                recorder.record(GetPlansMetric::new(response));
            }
        }
    }
}

#[uniffi_export]
pub fn send_payment_observability_metric(metric: PaymentObservabilityMetric) {
    let recorder = PreLoginMetricRecorder::default();
    metric.record(&recorder);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{self};

    use crate::core::observability::common::test_helper;

    #[test]
    fn test_iap_subscribe_metric() {
        const EVENT: &str = "payments_iap_subscribe_total";
        let serialized = test_helper::serialize_metric(IapSubscribeMetric {
            status: PaymentObservabilityResponse::Unknown,
        });

        assert_eq!(serialized, test_helper::json(EVENT));
        assert_eq!(
            test_helper::metric_request_element(EVENT),
            serde_json::de::from_str(&serialized).unwrap()
        );
    }

    #[test]
    fn test_send_payment_token_metric() {
        const EVENT: &str = "payments_iap_send_payment_token_total";
        let serialized = test_helper::serialize_metric(SendPaymentTokenMetric {
            status: PaymentObservabilityResponse::Unknown,
        });

        assert_eq!(serialized, test_helper::json(EVENT));
        assert_eq!(
            test_helper::metric_request_element(EVENT),
            serde_json::de::from_str(&serialized).unwrap()
        );
    }

    #[test]
    fn test_create_subscription_metric() {
        const EVENT: &str = "payments_iap_create_subscription_total";
        let serialized = test_helper::serialize_metric(CreateSubscriptionMetric {
            status: PaymentObservabilityResponse::Unknown,
        });

        assert_eq!(serialized, test_helper::json(EVENT));
        assert_eq!(
            test_helper::metric_request_element(EVENT),
            serde_json::de::from_str(&serialized).unwrap()
        );
    }

    #[test]
    fn test_get_subscription_metric() {
        const EVENT: &str = "payments_get_subscription_total";
        let serialized = test_helper::serialize_metric(GetSubscriptionMetric {
            status: PaymentObservabilityResponse::Unknown,
        });

        assert_eq!(serialized, test_helper::json(EVENT));
        assert_eq!(
            test_helper::metric_request_element(EVENT),
            serde_json::de::from_str(&serialized).unwrap()
        );
    }

    #[test]
    fn test_get_plans_metric() {
        const EVENT: &str = "payments_get_plans_total";
        let serialized = test_helper::serialize_metric(GetPlansMetric {
            status: PaymentObservabilityResponse::Unknown,
        });

        assert_eq!(serialized, test_helper::json(EVENT));
        assert_eq!(
            test_helper::metric_request_element(EVENT),
            serde_json::de::from_str(&serialized).unwrap()
        );
    }
}
