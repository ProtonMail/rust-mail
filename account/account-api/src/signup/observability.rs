use crate::ApiError;
use proton_core_api::services::observability::ApiServiceObservabilityResponse;

impl From<&ApiError> for ApiServiceObservabilityResponse {
    fn from(err: &ApiError) -> Self {
        match err {
            ApiError::Serialization(_) => ApiServiceObservabilityResponse::SerializationError,
            ApiError::Muon(_) => ApiServiceObservabilityResponse::NetworkError,
            ApiError::Status(status) => {
                if status.0.is_client_error() {
                    ApiServiceObservabilityResponse::Http4xx
                } else if status.0.is_server_error() {
                    ApiServiceObservabilityResponse::Http5xx
                } else {
                    ApiServiceObservabilityResponse::Unknown
                }
            }
            ApiError::Internal(_) => ApiServiceObservabilityResponse::Unknown,
        }
    }
}
