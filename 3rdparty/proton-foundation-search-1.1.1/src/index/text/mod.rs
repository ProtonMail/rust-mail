//! Text based index

mod text_one_blob;
mod text_with_buckets;

pub mod distance;
pub mod finding;
pub mod term;
pub mod token;
pub mod trigram;

#[cfg(any(test, debug_assertions, feature = "bench-mode"))]
pub use text_one_blob::TextIndexSansIo;
pub use text_with_buckets::TextIndex;
