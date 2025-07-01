mod data_impl;
mod request_data;
mod requests;

mod common;

pub use self::request_data::*;
pub use self::requests::*;
use crate::service::ApiServiceResult;
pub use common::*;

/// The Proton Data API base path (v1).
pub const DATA_V1: &str = "/data/v1";

#[allow(async_fn_in_trait)]
pub trait ProtonData {
    /// Asynchronously posts a batch of metrics to the observability endpoint.
    ///
    /// # Arguments
    /// * `body` - Vector of `PostMetricsRequestElement` structs representing metrics.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn post_metrics(&self, body: Vec<PostMetricsRequestElement>) -> ApiServiceResult<()>;
}
