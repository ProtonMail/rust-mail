use futures::future::try_join_all;
use itertools::Itertools;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_mail::services::proton::response_data::OperationResult;
use std::future::Future;

/// Repeatedly calls `endpoint` in batches of `limit` in parallel.
pub async fn split_request<F, Fut, R>(
    ids: impl IntoIterator<Item = R>,
    limit: usize,
    endpoint: F,
) -> Result<Vec<OperationResult>, ApiServiceError>
where
    F: Fn(Vec<ApiRemoteId>) -> Fut,
    Fut: Future<Output = Result<Vec<OperationResult>, ApiServiceError>>,
    R: Into<ApiRemoteId>,
{
    let chunks = ids.into_iter().map(R::into).chunks(limit);
    let ids = chunks.into_iter().map(|ids| endpoint(ids.collect_vec()));

    Ok(try_join_all(ids).await?.into_iter().flatten().collect())
}
