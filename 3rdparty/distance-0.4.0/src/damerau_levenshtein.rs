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

use std::collections::HashMap;
use std::char;

use utils;

/// Calculates the Damerau-Levenshtein distance between two strings.
/// 
/// # Damerau-Levenshtein distance
/// The [Damerau-Levenshtein distance](https://en.wikipedia.org/wiki/Damerau%E2%80%93Levenshtein_distance) is the number of per-character changes 
/// (insertion, deletion, substitution & transposition) that are neccessary to convert one string into annother.
/// The original Levenshtein distance does not take transposition into account.
/// This implementation does fully support unicode strings.
/// 
/// ## Complexity 
/// m := len(s) + 2  
/// n := len(t) + 2  
///
/// Time complexity:   O(mn)   
/// Space complexity:  O(mn + m) 
///
/// ## Examples
/// ```
/// use distance::*;
/// 
/// // Damerau-Levenshtein distance
/// let distance = damerau_levenshtein("hannah", "hannha");   
/// assert_eq!(1, distance);
/// ```
///
pub fn damerau_levenshtein(s: &str, t: &str) -> usize {
    // get length of unicode chars
    let len_s = s.chars().count();
    let len_t = t.chars().count();
    let max_distance = len_t + len_s;

    // initialize the matrix
    let mut mat: Vec<Vec<usize>> = vec![vec![0; len_t + 2]; len_s + 2];
    mat[0][0] = max_distance;
    for i in 0..(len_s + 1) {
        mat[i+1][0] = max_distance;
        mat[i+1][1] = i;
    }
    for i in 0..(len_t + 1) {
        mat[0][i+1] = max_distance;
        mat[1][i+1] = i;
    }

    let mut char_map: HashMap<char, usize> = HashMap::new();
    // apply edit operations
    for (i, s_char) in s.chars().enumerate() {
        let mut db = 0;
        let i = i + 1;
        
        for (j, t_char) in t.chars().enumerate() {
            let j = j + 1;
            let last = *char_map.get(&t_char).unwrap_or(&0);

            let cost = if s_char == t_char { 0 } else { 1 };
            mat[i+1][j+1] = utils::min4(
                mat[i+1][j] + 1,     // deletion
                mat[i][j+1] + 1,     // insertion 
                mat[i][j] + cost,    // substitution
                mat[last][db] + (i - last - 1) + 1 + (j - db - 1) // transposition
            );

            // that's like s_char == t_char but more efficient
            if cost == 0 {
                db = j;
            }
        }

        char_map.insert(s_char, i);
    }

    return mat[len_s + 1][len_t + 1];
}

#[cfg(test)]
mod tests {
    use super::damerau_levenshtein;

    #[test]
    fn basic() {
        assert_eq!(1, damerau_levenshtein("hannah", "hannha"));
        assert_eq!(2, damerau_levenshtein("FOO", "BOR"));
        assert_eq!(1, damerau_levenshtein("BAR", "BOR"));
        assert_eq!(1, damerau_levenshtein("hansi", "hasni"));
        assert_eq!(2, damerau_levenshtein("zzaabbio", "zzababoi"));
        assert_eq!(1, damerau_levenshtein("zzaabb", "zzabab"));
        assert_eq!(3, damerau_levenshtein("abcdef", "badcfe"));
        assert_eq!(1, damerau_levenshtein("klmb", "klm"));
        assert_eq!(1, damerau_levenshtein("klm", "klmb"));
    }

    #[test]
    fn empty() {
        assert_eq!(0, damerau_levenshtein("", ""));
        assert_eq!(3, damerau_levenshtein("", "foo"));
        assert_eq!(3, damerau_levenshtein("bar", ""));
    }

    #[test]
    fn cases() {
        assert_eq!(1, damerau_levenshtein("Hello", "hello"));
        assert_eq!(1, damerau_levenshtein("World", "world"));
    }

    #[test]
    fn same() {
        assert_eq!(0, damerau_levenshtein("pomme de terre", "pomme de terre"));
    }

    #[test]
    fn reversed() {
        assert_eq!(5, damerau_levenshtein("damerau", "iiqjau"));
        assert_eq!(5, damerau_levenshtein("iiqjau", "damerau"));
    }

    #[test]
    fn lorem() {
        assert_eq!(80, damerau_levenshtein(
            "Lorem ipsum dolor sit amet, autem mucius eirmod ea per. Nec adhuc laudem id, vix dolor interesset ea.", 
            "Id mundi ponderum constituam nam. Nam in legendos definiebas. Pri commune senserit omittantur cu.")
        );
    }

    #[test]
    fn unicode() {
        assert_eq!(6, damerau_levenshtein("さようなら", "༆˥ʘöpa"));
        assert_eq!(3, damerau_levenshtein("abc", "öঙ香"));
        assert_eq!(3, damerau_levenshtein("", "öঙ香"));
        assert_eq!(1, damerau_levenshtein("よさ", "äさ"));
    }

}
