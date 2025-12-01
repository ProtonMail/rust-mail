mod data_impl;
mod request_data;
mod requests;

mod common;

pub use self::request_data::*;
pub use self::requests::*;
use crate::service::ApiServiceResult;
pub use common::*;

pub const DATA_V1: &str = "/data/v1";

#[allow(async_fn_in_trait)]
pub trait ProtonData {
    async fn post_metrics(&self, body: Vec<PostMetricsRequestElement>) -> ApiServiceResult<()>;
}
