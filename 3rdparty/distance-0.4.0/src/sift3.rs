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

/// Calculates the sift3 distance between two strings with a default max_distance of 5.
/// 
/// # Sift3
/// [Sift3](http://siderite.blogspot.com/2007/04/super-fast-and-accurate-string-distance.html) - super fast and accurate string distance algorithm.
/// The higher the return value, the more different the two strings are. 
/// A value of 0.0 means both strings are equal.
///
/// This implementation does fully support unicode strings.
///
/// ## Examples
/// ```
/// use distance::*;
/// 
/// // Sift3 distance
/// let distance = sift3("hannah", "hanna");   
/// assert_eq!(0.5, distance);
/// ```
///
pub fn sift3(s: &str, t: &str) -> f32 {
    return sift3_offset(s, t, 5);
}

#[inline(always)]
fn sift3_offset(s: &str, t: &str, max_offset: usize) -> f32 {
    let len_s = s.chars().count();
    let len_t = t.chars().count();

    // handle empty strings
    if len_s == 0 {
        if len_t == 0 {
            return 0.0;
        } else {
            return len_t as f32;
        }
    }
    if len_t == 0 { 
        return len_s as f32; 
    }

    let sv: Vec<char> = s.chars().collect();
    let tv: Vec<char> = t.chars().collect();
    let mut c = 0;
    let mut offset1 = 0;
    let mut offset2 = 0;
    let mut lcs = 0;

    while (c + offset1 < len_s) && (c + offset2 < len_t) {
        if sv[c + offset1] == tv[c + offset2] {
            lcs += 1;
        } else {
            offset1 = 0;
            offset2 = 0;
            for i in 0..max_offset {
                if (c + i < len_s) && sv[c + i] == tv[c] {
                    offset1 = i;
                    break;
                } 

                if (c + i < len_t) && (sv[c] == tv[c + i]) {
                    offset2 = i;
                    break;
                }
            }
        }
        c += 1;
    }

    return ((len_s + len_t) as f32) / 2.0 - (lcs as f32);
}

#[cfg(test)]
mod tests {
    use super::sift3;

    #[test]
    fn basic() {
        assert_eq!(0.5, sift3("hannah", "hanna"));
        assert_eq!(2.5, sift3("kitten", "sitting"));
        assert_eq!(2.0, sift3("book", "back"));
        assert_eq!(4.5, sift3("table", "dinner"));
        assert_eq!(2.0, sift3("person", "pardon"));
        assert_eq!(0.5, sift3("person", "persons")); 
    }

    #[test]
    fn equal() {
        assert_eq!(0.0, sift3("kitten", "kitten"));
        assert_eq!(0.0, sift3("a", "a"));
        assert_eq!(0.0, sift3("ち", "ち"));
    }

    #[test]
    fn cases() {
        assert_eq!(1.0, sift3("Hello", "hello"));
        assert_eq!(1.0, sift3("World", "world"));
    }


    #[test]
    fn empty() {
        assert_eq!(4.0, sift3("book", ""));
        assert_eq!(4.0, sift3("", "book"));
        assert_eq!(0.0, sift3("", ""));
    }

    #[test]
    fn unicode() {
        assert_eq!(1.5, sift3("Späße", "Spaß"));
        assert_eq!(5.0, sift3("さようなら", "こんにちは"));
        assert_eq!(1.0, sift3("さようなら", "さようなう"));
        assert_eq!(2.0, sift3("こんにちは", "こんにちは abc"));
        assert_eq!(1.0, sift3("༆༃ʘ", "༆˥ʘ"));
    } 

    #[test]
    fn lorem() {
        assert_eq!(93.0, sift3(
            "Lorem ipsum dolor sit amet, autem mucius eirmod ea per. Nec adhuc laudem id, vix dolor interesset ea.", 
            "Id mundi ponderum constituam nam. Nam in legendos definiebas. Pri commune senserit omittantur cu.")
        );
    }

}