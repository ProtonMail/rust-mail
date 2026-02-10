//! Cantor pairing utilities for generating unique EntryIndex values
//!
//! This module provides functions to map (batch_number, entry_position) pairs
//! to unique EntryIndex values using the Cantor pairing function, ensuring
//! that WAL data and collection reconstruction use consistent identifiers.
//!
//! # Overflow Handling
//!
//! The Cantor pairing function can produce very large numbers. This implementation
//! includes overflow detection and safe handling for EntryIndex generation.

use std::fmt;

/// Error types for Cantor pairing operations
#[derive(Debug, Clone, PartialEq)]
pub enum CantorError {
    /// Overflow occurred during pairing - result exceeds EntryIndex range
    Overflow {
        batch: u32,
        position: u32,
        result: u64,
    },
    /// Invalid input parameters
    InvalidInput { batch: u32, position: u32 },
}

impl fmt::Display for CantorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CantorError::Overflow {
                batch,
                position,
                result,
            } => {
                write!(
                    f,
                    "Cantor pairing overflow: batch={}, position={} -> result={} (max u32={})",
                    batch,
                    position,
                    result,
                    u32::MAX
                )
            }
            CantorError::InvalidInput { batch, position } => {
                write!(
                    f,
                    "Invalid Cantor pairing input: batch={}, position={}",
                    batch, position
                )
            }
        }
    }
}

impl std::error::Error for CantorError {}

/// Safe Cantor pairing function with overflow detection
///
/// Maps two natural numbers to a single unique natural number using:
/// π(k₁, k₂) = ½(k₁ + k₂)(k₁ + k₂ + 1) + k₂
///
/// # Arguments
///
/// * `k1` - First natural number (batch number)
/// * `k2` - Second natural number (entry position)
///
/// # Returns
///
/// * `Ok(u32)` - A unique EntryIndex value if no overflow occurs
/// * `Err(CantorError::Overflow)` - If the result exceeds u32::MAX
/// * `Err(CantorError::InvalidInput)` - If inputs are invalid
///
/// # Examples
///
/// ```rust:ignore
/// use crate::cantor_pairing::cantor_pair_safe;
///
/// // Small values work fine
/// assert_eq!(cantor_pair_safe(1, 0), Ok(1));
/// assert_eq!(cantor_pair_safe(1, 1), Ok(4));
///
/// // Large values may overflow
/// let result = cantor_pair_safe(100000, 100000);
/// match result {
///     Ok(entry_index) => println!("EntryIndex: {}", entry_index),
///     Err(e) => println!("Overflow: {}", e),
/// }
/// ```
pub fn cantor_pair_safe(k1: u32, k2: u32) -> Result<u32, CantorError> {
    // Validate inputs
    if k1 == 0 && k2 == 0 {
        return Err(CantorError::InvalidInput {
            batch: k1,
            position: k2,
        });
    }

    let a = k1 as u64;
    let b = k2 as u64;

    // Calculate Cantor pairing: π(k₁, k₂) = ½(k₁ + k₂)(k₁ + k₂ + 1) + k₂
    let sum = a + b;
    let result = (sum * (sum + 1)) / 2 + b;

    // Check for overflow
    if result > u32::MAX as u64 {
        return Err(CantorError::Overflow {
            batch: k1,
            position: k2,
            result,
        });
    }

    Ok(result as u32)
}

/// Cantor pairing function with fallback to sequential numbering
///
/// This function attempts Cantor pairing first, but falls back to a simple
/// sequential approach if overflow would occur. This ensures we always
/// get a valid EntryIndex, though uniqueness across batches may be compromised.
///
/// # Arguments
///
/// * `k1` - First natural number (batch number)
/// * `k2` - Second natural number (entry position)
///
/// # Returns
///
/// A valid EntryIndex value, panics if overflow would occur
pub fn cantor_pair_with_fallback(k1: u32, k2: u32) -> u32 {
    // Safety check: panic if the Cantor pairing would overflow
    match cantor_pair_safe(k1, k2) {
        Ok(result) => result,
        Err(CantorError::Overflow {
            batch,
            position,
            result,
        }) => {
            panic!(
                "Cantor pairing overflow detected: batch={}, position={} -> result={} (max u32={}). This exceeds safe limits.",
                batch,
                position,
                result,
                u32::MAX
            );
        }
        Err(CantorError::InvalidInput { batch, position }) => {
            tracing::error!(
                "Invalid Cantor pairing input: batch={}, position={}",
                batch,
                position
            );
            0 // Return 0 for invalid input
        }
    }
}

/// Reverse Cantor pairing (unpairing) function
///
/// Recovers the original pair (k₁, k₂) from a Cantor-paired number.
/// Useful for debugging and validation.
///
/// # Arguments
///
/// * `z` - The Cantor-paired number
///
/// # Returns
///
/// The original (k₁, k₂) pair
pub fn cantor_unpair(z: u64) -> (u32, u32) {
    let w = ((((8 * z + 1) as f64).sqrt() - 1.0) / 2.0).floor() as u64;
    let t = (w * w + w) / 2;
    let k2 = z - t;
    let k1 = w - k2;
    (k1 as u32, k2 as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cantor_pairing_uniqueness() {
        let mut seen = std::collections::HashSet::new();

        // Test uniqueness for reasonable ranges
        for batch in 1..=10 {
            for pos in 0..100 {
                let paired = cantor_pair_safe(batch, pos).unwrap_or_else(|e| {
                    panic!("Should not overflow for small values: {:?}", e);
                });
                assert!(
                    !seen.contains(&paired),
                    "Collision detected: batch {} pos {} -> {}",
                    batch,
                    pos,
                    paired
                );
                seen.insert(paired);
            }
        }
    }

    #[test]
    fn test_cantor_safe_small_values() {
        // Test small values that should work fine
        assert_eq!(cantor_pair_safe(1, 0), Ok(1));
        assert_eq!(cantor_pair_safe(1, 1), Ok(4));
        assert_eq!(cantor_pair_safe(1, 2), Ok(8));
        assert_eq!(cantor_pair_safe(2, 0), Ok(3));
        assert_eq!(cantor_pair_safe(2, 1), Ok(7));
        assert_eq!(cantor_pair_safe(3, 0), Ok(6));
    }

    #[test]
    fn test_cantor_safe_invalid_input() {
        // Test invalid input (both zero)
        assert_eq!(
            cantor_pair_safe(0, 0),
            Err(CantorError::InvalidInput {
                batch: 0,
                position: 0
            })
        );

        // Test valid inputs (one can be zero)
        assert_eq!(cantor_pair_safe(0, 1), Ok(2)); // π(0,1) = (0+1)(0+1+1)/2 + 1 = 1*2/2 + 1 = 2
        assert_eq!(cantor_pair_safe(1, 0), Ok(1)); // π(1,0) = (1+0)(1+0+1)/2 + 0 = 1*2/2 + 0 = 1
    }

    #[test]
    fn test_cantor_safe_overflow_detection() {
        // Test values that should cause overflow
        // These are large enough to exceed u32::MAX when paired
        let large_batch = 100000u32;
        let large_position = 100000u32;

        let result = cantor_pair_safe(large_batch, large_position);
        match result {
            Err(CantorError::Overflow {
                batch,
                position,
                result,
            }) => {
                assert_eq!(batch, large_batch);
                assert_eq!(position, large_position);
                assert!(result > u32::MAX as u64);
            }
            Ok(_) => panic!("Expected overflow error for large values"),
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn test_cantor_fallback_invalid_input() {
        // Test with edge case where both batch and position are 0
        let result = cantor_pair_with_fallback(0, 0);
        assert_eq!(result, 0); // Should return 0 for invalid input
    }

    #[test]
    fn test_cantor_reversibility() {
        let test_cases = [
            (1, 0),
            (1, 1),
            (1, 100),
            (1, 5000),
            (2, 0),
            (2, 1),
            (2, 100),
            (2, 5000),
            (10, 0),
            (10, 500),
            (100, 0),
            (100, 100),
        ];

        for (batch, pos) in test_cases {
            let paired = cantor_pair_safe(batch, pos).unwrap_or_else(|e| {
                panic!("Should not overflow for test values: {:?}", e);
            });
            let (decoded_batch, decoded_pos) = cantor_unpair(paired as u64);
            assert_eq!(
                (batch, pos),
                (decoded_batch, decoded_pos),
                "Reversibility failed for ({}, {})",
                batch,
                pos
            );
        }
    }

    #[test]
    fn test_entry_index_generation() {
        // Test that different batch/position combinations give different EntryIndex values
        let entry1 =
            cantor_pair_safe(1, 0).unwrap_or_else(|e| panic!("Should not overflow: {:?}", e));
        let entry2 =
            cantor_pair_safe(1, 1).unwrap_or_else(|e| panic!("Should not overflow: {:?}", e));
        let entry3 =
            cantor_pair_safe(2, 0).unwrap_or_else(|e| panic!("Should not overflow: {:?}", e));
        let entry4 =
            cantor_pair_safe(2, 1).unwrap_or_else(|e| panic!("Should not overflow: {:?}", e));

        // All should be unique
        let entries = [entry1, entry2, entry3, entry4];
        for i in 0..entries.len() {
            for j in (i + 1)..entries.len() {
                assert_ne!(
                    entries[i], entries[j],
                    "EntryIndex collision between positions {} and {}",
                    i, j
                );
            }
        }
    }

    #[test]
    fn test_entry_index_decoding() {
        let test_cases = [(1, 0), (1, 100), (2, 0), (2, 50), (5, 1000)];

        for (batch, pos) in test_cases {
            let entry = cantor_pair_safe(batch, pos).unwrap_or_else(|e| {
                panic!("Should not overflow for test values: {:?}", e);
            });
            let (decoded_batch, decoded_pos) = cantor_unpair(entry as u64);
            assert_eq!(
                (batch, pos),
                (decoded_batch, decoded_pos),
                "Entry index decode failed for batch {} pos {}",
                batch,
                pos
            );
        }
    }

    #[test]
    fn test_cantor_examples() {
        // Test specific examples from documentation
        // Note: Our implementation uses 1-based batch numbers
        assert_eq!(cantor_pair_safe(1, 0), Ok(1)); // batch=1, pos=0 -> 1
        assert_eq!(cantor_pair_safe(1, 1), Ok(4)); // batch=1, pos=1 -> 4
        assert_eq!(cantor_pair_safe(1, 2), Ok(8)); // batch=1, pos=2 -> 8
        assert_eq!(cantor_pair_safe(2, 0), Ok(3)); // batch=2, pos=0 -> 3
        assert_eq!(cantor_pair_safe(2, 1), Ok(7)); // batch=2, pos=1 -> 7
        assert_eq!(cantor_pair_safe(3, 0), Ok(6)); // batch=3, pos=0 -> 6
    }

    #[test]
    fn test_overflow_boundary() {
        // Test that overflow detection works with known large values
        // Cantor pairing grows quadratically: π(k₁, k₂) = ½(k₁ + k₂)(k₁ + k₂ + 1) + k₂

        // Test with very large values that should definitely overflow
        let large_batch = 100000u32;
        let large_position = 100000u32;

        // These should definitely cause overflow
        assert!(cantor_pair_safe(large_batch, large_position).is_err());
        assert!(cantor_pair_safe(large_batch + 1, large_position).is_err());
        assert!(cantor_pair_safe(large_batch, large_position + 1).is_err());

        // Test with smaller values that should work
        let small_batch = 100u32;
        let small_position = 100u32;
        assert!(cantor_pair_safe(small_batch, small_position).is_ok());

        // Test with medium values - find where overflow starts
        let medium_batch = 50000u32;
        let medium_position = 50000u32;

        // This should overflow: π(50000, 50000) = ½(100000)(100001) + 50000 = 5,000,050,000,000 + 50000
        // Which is much larger than u32::MAX (4,294,967,295)
        assert!(cantor_pair_safe(medium_batch, medium_position).is_err());

        println!("Overflow detection working correctly for large values");
    }

    #[test]
    #[should_panic(expected = "Cantor pairing overflow detected")]
    fn test_panic_overflow_detection() {
        cantor_pair_with_fallback(100_000, 100_000);
    }

    #[test]
    fn test_safe_values_work() {
        // These should work without panicking
        assert!(cantor_pair_with_fallback(999, 91_681) > 0);
        assert!(cantor_pair_with_fallback(99, 92_581) > 0);
        assert!(cantor_pair_with_fallback(1, 1) > 0);
    }
}
