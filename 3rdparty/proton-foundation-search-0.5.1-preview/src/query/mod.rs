//! Search results.
//!
//! # Result Scoring
//!
//! Search results are scored based on:
//! - Term frequency in document
//! - Document length normalization
//! - Field type-specific scoring rules
//!
//! Scores are normalized between 0.0 and 1.0.
//!
//! # Result Ordering
//!
//! Results are ordered by:
//! 1. Score (descending)
//! 2. Document identifier (ascending, for consistent ordering of equal scores)

pub mod expression;
pub mod option;
pub mod results;
pub mod stats;
