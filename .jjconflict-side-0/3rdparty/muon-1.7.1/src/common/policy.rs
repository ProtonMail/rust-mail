//! ## Policy
//!
//! This module defines policy types by which requests will be handled.

use crate::util::DurationExt;
use derive_more::{Add, From, Into};
use rand::Rng;
use std::time::Duration;

/// A request type of service.
#[must_use]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ServiceType {
    /// The request is important for the client UI to work properly, and should
    /// be sent as fast as possible.
    ///
    /// Examples: login, sign-up, fetching data not in cache
    Interactive = 2,
    /// The request is not driving the UI, but still of relative importance and
    /// has some kind of indirect impact on user experience
    ///
    /// Examples: user triggered actions that we have cached data for or that we
    /// can retry later
    #[default]
    Normal = 3,
    /// Background request, can be delayed significantly if needed, but still
    /// impacts the user experience.
    ///
    /// Examples: cache refresh
    Background = 4,
    /// The request is not driving the UI and is not important for the user
    /// experience.
    ///
    /// Example: metrics
    Optional = 6,
}

/// The retry policy of a request.
///
/// This policy defines how a request should be retried in case of failure.
/// It also implements `IntoIterator` to allow the policy to be used as an
/// iterator which yields the delays between retries.
#[must_use]
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// The number of times to retry the request, 0 for no retries.
    pub max_count: usize,

    /// The minimum delay between retries.
    pub min_delay: Duration,

    /// The maximum delay between retries.
    pub max_delay: Duration,

    /// The arithmetic progression of the delay between retries.
    /// That is, the delay between retries will be increased by this amount.
    pub iter_add: Duration,

    /// The geometric progression of the delay between retries.
    /// That is, the delay between retries will be multiplied by this amount.
    pub iter_mul: f64,

    /// The amount of random jitter to apply to the delay.
    pub jitter: Duration,
}

/// TODO: Are there any non-idempotent API calls?
impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            max_count: 3,
            min_delay: 1.s(),
            max_delay: 30.s(),
            iter_add: 0.s(),
            iter_mul: 2.0,
            jitter: 3.s(),
        }
    }
}

impl RetryPolicy {
    /// Never retry a request.
    pub fn never(self) -> Self {
        self.max_count(0)
    }

    /// Set the max retry count.
    pub fn max_count(self, max_count: usize) -> Self {
        Self { max_count, ..self }
    }

    /// Set the minimum delay between retries.
    pub fn min_delay(self, min_delay: Duration) -> Self {
        Self { min_delay, ..self }
    }

    /// Set the maximum delay between retries.
    pub fn max_delay(self, max_delay: Duration) -> Self {
        Self { max_delay, ..self }
    }

    /// Set the amount to add to the delay between retries.
    pub fn iter_add(self, iter_add: Duration) -> Self {
        Self { iter_add, ..self }
    }

    /// Set the multiplier of the delay between retries.
    pub fn iter_mul(self, iter_mul: f64) -> Self {
        Self { iter_mul, ..self }
    }

    /// Set the maximum amount of random jitter to apply to the delay.
    ///
    /// The jitter is a random value between 0 and the given value
    /// which is added to the delay between retries.
    pub fn jitter(self, jitter: Duration) -> Self {
        Self { jitter, ..self }
    }
}

/// Convert a retry policy into an iterator.
impl IntoIterator for RetryPolicy {
    type Item = Duration;
    type IntoIter = RetryPolicyIter;

    fn into_iter(self) -> Self::IntoIter {
        RetryPolicyIter::new(self)
    }
}

/// Iterator for generating retries based on a retry policy.
#[derive(Debug, Clone)]
pub struct RetryPolicyIter {
    p: RetryPolicy,
    d: Duration,
    n: usize,
}

impl RetryPolicyIter {
    /// Create a new retry policy.
    #[must_use]
    pub fn new(policy: RetryPolicy) -> Self {
        RetryPolicyIter {
            p: policy,
            d: policy.min_delay,
            n: 0,
        }
    }
}

impl Iterator for RetryPolicyIter {
    type Item = Duration;

    fn next(&mut self) -> Option<Self::Item> {
        (self.n < self.p.max_count).then(|| {
            let delay = self.d;

            self.d = self.d.add(self.p.iter_add);
            self.d = self.d.mul_f64(self.p.iter_mul);
            self.d = self.d.min(self.p.max_delay);
            self.n += 1;

            delay + jitter(self.p.jitter)
        })
    }
}

/// The cost of servicing a request.
#[derive(Debug, From, Into, Clone, Copy)]
pub struct ServiceCost {
    /// The cost of the request.
    pub cost: u32,
}

impl Default for ServiceCost {
    fn default() -> Self {
        ServiceCost { cost: 1000 }
    }
}

/// Generate a random jitter value between 0 and the given maximum.
fn jitter(max: Duration) -> Duration {
    if let Ok(ns) = rand::thread_rng().gen_range(0..=max.as_nanos()).try_into() {
        Duration::from_nanos(ns)
    } else {
        max
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::IntoIterExt;
    use itertools::Itertools;

    #[test]
    fn test_jitter_bounds() {
        assert!((0..100)
            .map(|_| jitter(1.s()))
            .all(|j| 0.s() <= j && j <= 1.s()));
    }

    #[test]
    fn test_retry_policy_iter_mul() {
        let p = RetryPolicy::default()
            .min_delay(1.s())
            .max_count(7)
            .jitter(0.s());

        assert_eq!(
            p.into_vec(),
            vec![
                1.s(), // starts at min_delay
                2.s(), // doubles every iteration
                4.s(),
                8.s(),
                16.s(),
                30.s(), // up to max_delay (default 30s)
                30.s(), // stays at max_delay
            ]
        );
    }

    #[test]
    fn test_retry_policy_iter_add() {
        let p = RetryPolicy::default()
            .min_delay(1.s())
            .max_delay(4.s())
            .max_count(7)
            .iter_add(1.s())
            .iter_mul(1.0)
            .jitter(0.s());

        assert_eq!(
            p.into_vec(),
            vec![
                1.s(), // starts at min_delay
                2.s(), // adds iter_add every iteration
                3.s(),
                4.s(), // up to max_delay
                4.s(), // stays at max_delay
                4.s(),
                4.s(),
            ]
        );
    }

    #[test]
    fn test_retry_policy_iter_jitter() {
        // By default, jitter is applied: no progressions should be the same.
        let p = RetryPolicy::default().max_count(5);
        let d = (0..100).map(|_| p.into_vec()).into_vec();
        assert_eq!(d.iter().unique().count(), d.len());

        // // But if we disable jitter, they should all be the same.
        let p = p.jitter(0.s());
        let d = (0..100).map(|_| p.into_vec()).into_vec();
        assert_eq!(d.iter().unique().count(), 1);
    }
}
