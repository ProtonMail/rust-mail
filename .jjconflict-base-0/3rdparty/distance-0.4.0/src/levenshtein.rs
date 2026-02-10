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

use utils;

/// Calculates the Levenshtein distance between two strings.
/// 
/// # Levenshtein distance
/// The [Levenshtein distance](https://en.wikipedia.org/wiki/Levenshtein_distance) is the number of per-character changes (insertion, deletion & substitution)
/// that are neccessary to convert one string into annother.
/// This implementation does fully support unicode strings.
///
/// ## Complexity 
/// m := len(s) + 1  
/// n := len(t) + 1  
///
/// Time complexity:   O(mn)   
/// Space complexity:  O(mn)
///
/// ## Examples
/// ```
/// use distance::*;
/// 
/// // Levenshtein distance
/// let distance = levenshtein("hannah", "hanna");   
/// assert_eq!(1, distance);
/// ```
///
pub fn levenshtein(s: &str, t: &str) -> usize {
    // get length of unicode chars
    let len_s = s.chars().count();
    let len_t = t.chars().count();

    // initialize the matrix
    let mut mat: Vec<Vec<usize>> = vec![vec![0; len_t + 1]; len_s + 1];
    for i in 1..(len_s + 1) { 
        mat[i][0] = i; 
    }
    for i in 1..(len_t + 1) { 
        mat[0][i] = i; 
    }

    // apply edit operations
    for (i, s_char) in s.chars().enumerate() {
        for (j, t_char) in t.chars().enumerate() {
            let substitution = if s_char == t_char {0} else {1};
            mat[i+1][j+1] = utils::min3(
                mat[i][j+1] + 1,            // deletion
                mat[i+1][j] + 1,            // insertion 
                mat[i][j] + substitution    // substitution
            );
        }
    }

    return mat[len_s][len_t];
}

#[cfg(test)]
mod tests {
    use super::levenshtein;

    #[test]
    fn basic() {
        assert_eq!(3, levenshtein("kitten", "sitting"));
        assert_eq!(2, levenshtein("book", "back"));
        assert_eq!(5, levenshtein("table", "dinner"));
        assert_eq!(2, levenshtein("person", "pardon"));
        assert_eq!(1, levenshtein("person", "persons"));
    }

    #[test]
    fn equal() {
        assert_eq!(0, levenshtein("kitten", "kitten"));
        assert_eq!(0, levenshtein("a", "a"));
    }

    #[test]
    fn cases() {
        assert_eq!(1, levenshtein("Hello", "hello"));
        assert_eq!(1, levenshtein("World", "world"));
    }

    #[test]
    fn empty() {
        assert_eq!(4, levenshtein("book", ""));
        assert_eq!(4, levenshtein("", "book"));
        assert_eq!(0, levenshtein("", ""));
    }

    #[test]
    fn unicode() {
        assert_eq!(2, levenshtein("Späße", "Spaß"));
        assert_eq!(5, levenshtein("さようなら", "こんにちは"));
        assert_eq!(1, levenshtein("さようなら", "さようなう"));
        assert_eq!(4, levenshtein("こんにちは", "こんにちは abc"));
        assert_eq!(1, levenshtein("༆༃ʘ", "༆˥ʘ"));
    }

}