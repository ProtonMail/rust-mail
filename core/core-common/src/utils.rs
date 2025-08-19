#[cfg(test)]
#[path = "tests/utils.rs"]
mod tests;

use base64::{Engine, prelude::BASE64_STANDARD};
use proton_crypto::generate_secure_random_bytes;
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
