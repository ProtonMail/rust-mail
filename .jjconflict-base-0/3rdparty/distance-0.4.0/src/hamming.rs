// Copyright (c) 2016. See AUTHORS file.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::DistanceError;

/// Calculates the Hamming distance between two strings of equal length.
///
/// # Hamming distance 
/// The [Hamming distance](https://en.wikipedia.org/wiki/Hamming_distance) between two strings of equal length is the number of positions at 
/// which two strings are different.
/// This returns an error of type DistanceError::InvalidArgs if the string arguments do not have equal length.
/// This implementation does fully support unicode strings.
///
/// ## Complexity 
/// Time complexity: O(n)
/// Space complexity: O(1)
///
/// ## Examples
/// ```
/// use distance::*;
/// 
/// // Hamming distance
/// let distance = hamming("karolin", "kathrin").unwrap();   
/// assert_eq!(3, distance);
/// ```
///
pub fn hamming(s: &str, t: &str) -> Result<usize, DistanceError> {
    if s.chars().count() != t.chars().count() {
        return Err(DistanceError::InvalidArgs);
    }

    let mut distance = 0;
    for (s_char, t_char) in s.chars().zip(t.chars()) {
        if s_char != t_char { 
            distance += 1
        }
    }

    return Ok(distance);
}

#[cfg(test)]
mod tests {
    use super::hamming;
    use ::DistanceError;

    #[test]
    fn basic() {
        assert_eq!(0, hamming("sitting", "sitting").unwrap());
        assert_eq!(7, hamming("abcdefg", "hijklmn").unwrap());
        assert_eq!(3, hamming("karolin", "kathrin").unwrap());
        assert_eq!(4, hamming("hello", "world").unwrap());
        assert_eq!(1, hamming("Rust", "rust").unwrap());
    }

    #[test]
    #[allow(unused_variables)]
    fn matching() {
        match hamming("karolin", "kathrin") {
            Ok(distance) => assert_eq!(3, distance),
            Err(err) => panic!("This should not happen"), 
        };

        match hamming("abra", "kadabra") {
            Ok(distance) => panic!("These are not valid inputs"),
            Err(err) => assert_eq!(err, DistanceError::InvalidArgs),
        };
    }

    #[test]
    #[should_panic]
    fn different_len() {
        hamming("abra", "kadabra").unwrap();
    }

    #[test]
    fn empty() {
        assert_eq!(0, hamming("", "").unwrap());
    }

    #[test]
    fn unicode() {
        assert_eq!(1, hamming("さようなら", "さようなは").unwrap());
        assert_eq!(1, hamming("säge", "sage").unwrap());
        assert_eq!(4, hamming("ßäöüé", "ößäüè").unwrap());
    }

    #[test]
    #[should_panic]
    fn unicode_different_len() {
        hamming("さようならa", "さようなら").unwrap();
    }

}