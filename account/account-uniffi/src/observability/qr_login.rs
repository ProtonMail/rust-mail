use proton_core_api::{
    metric,
    services::observability::{ObservabilityMetric, ObservabilityRecorder},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum QrLoginScanScreenViewTotalScreenId {
    #[serde(rename = "origin_instructions")]
    Instructions,
    #[serde(rename = "origin_qr_camera")]
    Camera,
    #[serde(rename = "origin_veryfying")]
    Verifying,
    #[serde(rename = "origin_success")]
    Success,
    #[serde(rename = "origin_failure")]
    Failure,
    #[serde(rename = "origin_no_camera_permission")]
    CameraAccessNotAllowed,
}

metric! {
    #[name = "core_qr_login_scan_screen_total"]
    #[version = 1]
    #[doc = "This metric type records the possible outcomes of the QR Login Host/Origin device screens."]
    pub struct QrLoginScanScreenViewTotal {
        pub screen_id: QrLoginScanScreenViewTotalScreenId,
    }
}

#[uniffi_export]
pub fn qr_login_scan_screen_total(screen_id: QrLoginScanScreenViewTotalScreenId) {
    ObservabilityRecorder::default().record(QrLoginScanScreenViewTotal::new(screen_id), true);
}

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum QrLoginShowQrCodeScreenViewTotalScreenId {
    #[serde(rename = "target_instructions")]
    Instructions,
    #[serde(rename = "target_failure")]
    Failure,
}

metric! {
    #[name = "core_qr_login_show_qr_code_screen_total"]
    #[version = 1]
    #[doc = "This metric type records the possible outcomes of the QR Login Target device screens."]
    pub struct QrLoginShowQrCodeScreenViewTotal {
        pub screen_id: QrLoginShowQrCodeScreenViewTotalScreenId,
    }
}

#[uniffi_export]
pub fn qr_login_show_qr_screen_total(screen_id: QrLoginShowQrCodeScreenViewTotalScreenId) {
    ObservabilityRecorder::default().record(QrLoginShowQrCodeScreenViewTotal::new(screen_id), true);
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_qr_host_metric() {
        let metric = ObservabilityRecorder::into_metrics_element(
            QrLoginScanScreenViewTotal {
                screen_id: QrLoginScanScreenViewTotalScreenId::CameraAccessNotAllowed,
            },
            1_741_021_308,
            1,
        )
        .unwrap();

        let serialized = serde_json::to_string(&metric).unwrap();
        assert_eq!(
            serialized,
            r#"{"Name":"core_qr_login_scan_screen_total","Version":1,"Timestamp":1741021308,"Data":{"Labels":{"screen_id":"origin_no_camera_permission"},"Value":1}}"#
        );
    }

    #[test]
    fn test_qr_target_metric() {
        let metric = ObservabilityRecorder::into_metrics_element(
            QrLoginShowQrCodeScreenViewTotal {
                screen_id: QrLoginShowQrCodeScreenViewTotalScreenId::Instructions,
            },
            1_741_021_308,
            1,
        )
        .unwrap();

        let serialized = serde_json::to_string(&metric).unwrap();
        assert_eq!(
            serialized,
            r#"{"Name":"core_qr_login_show_qr_code_screen_total","Version":1,"Timestamp":1741021308,"Data":{"Labels":{"screen_id":"target_instructions"},"Value":1}}"#
        );
    }

    #[test]
    fn test_observability_methods_without_async_runtime_no_panic() {
        qr_login_scan_screen_total(QrLoginScanScreenViewTotalScreenId::Success);
        qr_login_show_qr_screen_total(QrLoginShowQrCodeScreenViewTotalScreenId::Instructions);
    }
}
