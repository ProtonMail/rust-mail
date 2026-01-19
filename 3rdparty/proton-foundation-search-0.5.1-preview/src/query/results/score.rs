use core::f64;
use std::cmp::Ordering;
use std::hash::Hash;
use std::ops::Deref;

use serde::{Deserialize, Serialize};
use tracing::error;

use crate::query::expression::Operator;

/// Float number representing relative matching score
/// The score is guarranteed to be within the (0.0..=1.0) range
/// 0.0 represents a match without a score
/// NAN, INFINITY are not valid scores
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Score(f64);
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl Score {
    /// Checks the score and panics on NaN or out of bounds
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(constructor)
    )]
    pub fn new(score: f64) -> Self {
        let is_nan = score.is_nan();
        let is_valid = is_nan || (0.0..=1.0).contains(&score);
        if !is_valid {
            error!("Invalid score {score}")
        }
        if is_nan {
            Self(0.0)
        } else {
            Self(score.clamp(0.0, 1.0))
        }
    }

    /// Create accuracy, guaranteeing range 0.0 - 1.0 with 1.0 returned for zero denominator
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "fraction")
    )]
    pub fn new_fraction(nominator: usize, denominator: usize) -> Self {
        if denominator == 0 {
            return Score::EXACT;
        }
        Self((nominator as f64 / denominator as f64).clamp(0.0, 1.0))
    }

    /// Get the score value, which is guarranteed to be within the (0.0..=1.0) range
    pub fn value(&self) -> f64 {
        self.0
    }

    /// Round the score to given decimals
    pub fn round(&self, decimals: u8) -> Score {
        let mul = 10f64.powf(decimals as f64);
        Score((self.0 * mul).round() / mul)
    }

    /// Merge the two scores:
    ///
    /// OR => highest score
    /// AND => lowest score except for unscored 0.0
    pub fn merge(&mut self, op: Operator, other: Score) {
        let a = self.0;
        let b = other.0;
        self.0 = match op {
            Operator::Or => a.max(b),
            Operator::And => match (a, b) {
                (0.0, either) | (either, 0.0) => either,
                _ => a.min(b),
            },
        }
    }
}

impl Score {
    /// A score value representing an exact match
    pub const EXACT: Score = Score(1.0);
    /// A score of an unmatched condition
    pub const NONE: Score = Score(0.0);
}

impl From<f64> for Score {
    fn from(value: f64) -> Self {
        Self::new(value)
    }
}

impl From<Score> for f64 {
    fn from(value: Score) -> Self {
        value.0
    }
}

impl Hash for Score {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_ne_bytes().hash(state);
    }
}

impl Ord for Score {
    fn cmp(&self, other: &Self) -> Ordering {
        (-self.0).total_cmp(&-other.0)
    }
}

impl PartialOrd for Score {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Score {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Score {}

impl Deref for Score {
    type Target = f64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for Score {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::ops::Mul for Score {
    type Output = Score;

    fn mul(self, rhs: Self) -> Self::Output {
        Self(self.0 * rhs.0)
    }
}

#[test]
fn zero_on_nan() {
    assert_eq!(Score::new(f64::NAN).value(), 0.0);
    assert_eq!(Score::new(-f64::NAN).value(), 0.0);
}

#[test]
fn extreme_fraction() {
    assert_eq!(Score::new_fraction(usize::MAX, 0).value(), 1.0);
    assert_eq!(Score::new_fraction(usize::MAX, 1).value(), 1.0);
    assert_eq!(Score::new_fraction(0, usize::MAX).value(), 0.0);
    assert_eq!(
        Score::new_fraction(1, usize::MAX).value(),
        5.421010862427522e-20
    );
}

#[test]
fn reflexive_equality() {
    for sut in [
        Score::new(f64::NAN),
        Score::new(f64::INFINITY),
        Score::new(f64::NEG_INFINITY),
        Score::new(0.0),
        Score::new(-0.0),
        Score::new(-0.1),
        Score::new(0.424_242_424_242_424_25),
    ] {
        let other = sut;
        assert_eq!(sut, other);
    }
}

#[test]
fn invalid_values() {
    let sut = Score::new(-0.0);
    assert_eq!(sut.value(), 0.0);
    let sut = Score::new(-0.5);
    assert_eq!(sut.value(), 0.0);
    let sut = Score::new(f64::INFINITY);
    assert_eq!(sut.value(), 1.0);
}

#[test]
fn valid_values() {
    let sut = Score::new(f64::NAN);
    assert_eq!(sut.value(), 0.0);
    let sut = Score::new(0.0);
    assert_eq!(sut.value(), 0.0);
    let sut = Score::new(0.5);
    assert_eq!(sut.value(), (0.5));
    let sut = Score::new(1.0);
    assert_eq!(sut.value(), (1.0));
    let sut = Score::new(3.0);
    assert_eq!(
        sut.value(),
        (1.0),
        "values above 1.0 are invalid, but clamped to 1.0"
    );
}

#[test]
fn compares() {
    let sut_worse = Score::new(0.1);
    let sut_better = Score::new(0.9);
    assert_eq!(sut_worse.cmp(&sut_better), Ordering::Greater);
}
