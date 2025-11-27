#[cfg(test)]
#[path = "tests/utils.rs"]
mod tests;

use std::time::Instant;

use base64::{Engine, prelude::BASE64_STANDARD};
pub use proton_api_utils::{PaginateOptions, PaginateResponse};
use proton_core_api::MAX_PAGE_ELEMENT_COUNT;
use proton_crypto::generate_secure_random_bytes;
use tokio::task::JoinSet;
use unicode_segmentation::UnicodeSegmentation;

/// Returns the first grapheme of the string in uppercase.
/// Graphene is a user-perceived character, which can be any Unicode code point.
///
#[must_use]
pub fn first_grapheme_upppercase<S: AsRef<str>>(s: S) -> Option<String> {
    Some(s.as_ref().trim().graphemes(true).next()?.to_uppercase())
}

/// List of Proton colors defined by designers.
static PROTON_COLORS: [&str; 15] = [
    "#2E8378", // Green-1 (Genoa)
    "#34A48A", // Green-2 (Gossamer)
    "#52CD96", // Green-3 (Mountain Meadow)
    "#51BE50", // Green-4 (Apple)
    "#3F8B8E", // Green-5 (Paradiso)
    "#764AC4", // Purple-1 (Royal Purple)
    "#9E66FC", // Purple-2 (Heliotrope)
    "#9C89FF", // Purple-3 (Melrose)
    "#A1439F", // Purple-4 (Medium Red Violet)
    "#7B3185", // Purple-5 (Ripe Plum)
    "#495EA9", // Blue-1 (Bay of Many)
    "#4E7ABB", // Blue-2 (Cobalt)
    "#4989FF", // Blue-3 (Dodger Blue)
    "#3FB0D9", // Blue-4 (Picton Blue)
    "#4F66DF", // Blue-5 (Royal Blue)
];

/// Returns hexadecimal Proton color based on string value.
///
#[must_use]
pub fn proton_color(name: &str) -> &str {
    let mut hash = 0;
    for c in name.chars() {
        hash = (c as u32 + ((hash << 5) - hash)) % (65537);
    }
    let index = hash as usize % PROTON_COLORS.len();
    PROTON_COLORS[index]
}

/// This is a utility ergonomic trait as a shorthand for doing
/// `foo.into_iter().map(Into::into).collect::<Vec<_>>()`
pub trait MapVec<A> {
    fn map_vec(self) -> A;
}

impl<T: IntoIterator<Item = B>, A, B> MapVec<Vec<A>> for T
where
    B: Into<A>,
{
    fn map_vec(self) -> Vec<A> {
        self.into_iter().map(Into::into).collect()
    }
}

impl<T: IntoIterator<Item = B>, A, B> MapVec<Option<Vec<A>>> for Option<T>
where
    B: Into<A>,
{
    fn map_vec(self) -> Option<Vec<A>> {
        self.map(MapVec::map_vec)
    }
}

const NONCE_SIZE: usize = 32;

/// Generate a random nonce for the sake of Content Security Policy.
/// <https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy#nonce-nonce_value/>
#[must_use]
pub fn generate_csp_nonce() -> String {
    let bytes: [u8; NONCE_SIZE] = generate_secure_random_bytes();
    BASE64_STANDARD.encode(bytes)
}

#[allow(async_fn_in_trait)]
pub trait Paginatable {
    type PaginateOptions: PaginateOptions + Clone + Send + 'static;
    type Response: PaginateResponse<Self::Output>;
    type Output: Sized + Send + 'static;
    type Error: Send + 'static;
    type API: Send + Clone + 'static;
    const NAME: &'static str;
    const DEFAULT_PAGE_SIZE: u64 = MAX_PAGE_ELEMENT_COUNT;

    fn fetch(
        api: &Self::API,
        options: Self::PaginateOptions,
    ) -> impl std::future::Future<Output = Result<Self::Response, Self::Error>> + std::marker::Send;

    async fn fetch_all_filtered(
        api: &Self::API,
        options: Self::PaginateOptions,
    ) -> Result<Vec<Self::Output>, Self::Error> {
        // In order to maximize throughput we do as follows:
        // 1. We download the first batch
        // 2. We calculate how many batches are left and request them all in parallel.
        let t0 = Instant::now();

        let page_size = options.size();
        let first_response = Self::fetch(api, options.clone()).await?;

        tracing::debug!("Requested initial batch in {:?}", t0.elapsed());

        let mut joinset = JoinSet::new();
        if let Some(rem) = first_response.total().checked_sub(page_size) {
            let rem = rem.div_ceil(page_size);
            tracing::debug!("Requesting {rem} batches for {}", Self::NAME);
            for page in 1..=rem {
                let options = options.clone().with_page(page);
                let api = api.clone();
                joinset.spawn(async move {
                    Self::fetch(&api, options)
                        .await
                        .map(PaginateResponse::items)
                });
            }
        }

        let rest = joinset.join_all().await;

        let result: Vec<_> = std::iter::once(Ok(first_response.items()))
            .chain(rest)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();

        tracing::debug!(
            "Fetched {} {} in {:?}",
            result.len(),
            Self::NAME,
            t0.elapsed()
        );
        Ok(result)
    }

    async fn fetch_all(api: &Self::API) -> Result<Vec<Self::Output>, Self::Error> {
        Self::fetch_all_filtered(
            api,
            Self::PaginateOptions::from_zero(Self::DEFAULT_PAGE_SIZE),
        )
        .await
    }
}
