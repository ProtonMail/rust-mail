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

//! # A collection of approximate string matching algorithms 
//!
//! This library contains algorithms dealing with [approximate string matching](https://en.wikipedia.org/wiki/Approximate_string_matching).
//! These algorithms can be used to tell the approximate difference between two 
//! strings. This is usful for a varity of things like spell checking, fuzzy search, etc.
//! 
//! ## Algorithms
//! - [Levenshtein distance](https://en.wikipedia.org/wiki/Levenshtein_distance)
//! - [Damerau–Levenshtein distance](https://en.wikipedia.org/wiki/Damerau%E2%80%93Levenshtein_distance)
//! - [Hamming distance](https://en.wikipedia.org/wiki/Hamming_distance)
//! - [Sift3 distance](http://siderite.blogspot.com/2007/04/super-fast-and-accurate-string-distance.html)
//!
pub use self::levenshtein::*;
pub use self::damerau_levenshtein::*;
pub use self::hamming::*;
pub use self::sift3::*;
pub use self::errors::*;

mod levenshtein;
mod sift3;
mod hamming;
mod errors;
mod damerau_levenshtein;
mod utils;