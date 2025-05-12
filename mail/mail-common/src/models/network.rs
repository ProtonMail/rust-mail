use futures::future::try_join_all;
use itertools::Itertools;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::ProtonIdMarker;
use proton_mail_api::services::proton::response_data::OperationResult;
use std::future::Future;

/// Repeatedly calls `endpoint` in batches of `limit` in parallel.
pub async fn split_request<F, Fut, R>(
    ids: impl IntoIterator<Item = R>,
    limit: usize,
    endpoint: F,
) -> Result<Vec<OperationResult<R>>, ApiServiceError>
where
    F: Fn(Vec<R>) -> Fut,
    Fut: Future<Output = Result<Vec<OperationResult<R>>, ApiServiceError>>,
    R: ProtonIdMarker,
{
    let chunks = ids.into_iter().chunks(limit);
    let ids = chunks.into_iter().map(|ids| endpoint(ids.collect_vec()));

    Ok(try_join_all(ids).await?.into_iter().flatten().collect())
}
