//! Helpers for implementing keyword grouping

use crate::{IParse, Ident, ToTokens};

#[cfg(feature = "hash_keywords")]
use fxhash::FxHashSet;

// Arbitrarily chosen based on guessed usage.  Profiling this wont make much sense because the
// actual kind and usage of keywords in the parsed source code will make a difference in
// performance.
/// The break even between linear search and building a hash table/binary search.
const MAX_LINEAR_SEARCH: usize = 4;

// Implementation detail only exposed because the keyword macro needs it
/// Statically constructed tree of `&str` grouping keywords
#[doc(hidden)]
pub enum KeywordGroup {
    List(&'static [&'static KeywordGroup]),
    Keyword(&'static str),
}

impl KeywordGroup {
    /// Creates an iterator over the keywords in this group.
    #[allow(clippy::iter_without_into_iter)] // don't need, this is a internal API
    fn iter(&self) -> KeywordGroupIter {
        let KeywordGroup::List(nodes) = self else {
            // Can only iterate KeywordGroup::List, the macros ensure this,
            // will never happen in practice.
            panic!();
        };
        // preallocating 8 elements means that this vec won't need to grow in most cases and
        // is pretty cheap since these allocations are short lived.
        let mut vec = Vec::with_capacity(8);
        vec.push(nodes.iter());
        KeywordGroupIter(vec)
    }

    fn assert_identifiers(&self) {
        for token in self.iter() {
            assert!(
                token.to_token_iter().parse::<Ident>().is_ok(),
                "'{token}' is not a valid keyword"
            );
        }
    }

    /// count objects up to `max` in the iterator return the count
    fn bounded_len(&self, mut max: usize) -> usize {
        let mut count = 0usize;
        let mut iter = self.iter();
        while iter.next().is_some() && max > 0 {
            count += 1;
            max -= 1;
        }
        count
    }
}

struct KeywordGroupIter(Vec<std::slice::Iter<'static, &'static KeywordGroup>>);

impl Iterator for KeywordGroupIter {
    type Item = &'static str;

    #[mutants::skip] // mutating this sends it into a death-loop
    fn next(&mut self) -> Option<&'static str> {
        while let Some(tree) = self.0.last_mut() {
            match tree.next() {
                Some(KeywordGroup::List(nodes)) => self.0.push(nodes.iter()),
                Some(KeywordGroup::Keyword(s)) => {
                    return Some(s);
                }
                None => {
                    self.0.pop();
                }
            }
        }
        None
    }
}

/// The static tree representation how keyword groups are defined by macros is not ideal for
/// fast lookup. We only use it to lazily construct optimized match functions.
#[doc(hidden)]
#[must_use]
#[mutants::skip]
pub fn create_matchfn(group: &'static KeywordGroup) -> Box<dyn Fn(&str) -> bool + Send + Sync> {
    group.assert_identifiers();

    match group.bounded_len(MAX_LINEAR_SEARCH + 1) {
        #[cfg(debug_assertions)]
        // should never ever happen because the macros ensure at least one element.
        // If it happens in 'release' mode then matching on _ later is harmless.
        0 => {
            panic!("empty KeywordGroup")
        }

        // optimization for exactly one string
        1 => {
            let Some(s) = group.iter().next() else {
                // we just determined that we have length 1
                panic!()
            };
            Box::new(move |this| this == s)
        }

        // a few strings are linear searched instead being hashed/binary_searched
        len @ 2..=MAX_LINEAR_SEARCH => {
            let mut array = [""; MAX_LINEAR_SEARCH];
            for (index, element) in group.iter().enumerate() {
                array[index] = element;
            }
            Box::new(move |this| array[..len].contains(&this))
        }

        // any more strings looked up in a HashSet when `hash_keywords` is set
        #[cfg(feature = "hash_keywords")]
        _ => {
            let hash = group.iter().collect::<FxHashSet<_>>();
            Box::new(move |this| hash.contains(&this))
        }

        // any more strings looked up from a sorted Vec when `hash_keywords` is not set
        #[cfg(not(feature = "hash_keywords"))]
        _ => {
            let mut vec = group.iter().collect::<Vec<_>>();
            vec.sort_unstable();
            // keyword groups may contain duplicates
            vec.dedup();

            Box::new(move |this| vec.binary_search(&this).is_ok())
        }
    }
}

#[cfg(test)]
mod test {
    use crate::*;

    keyword! {
            If = "if";
            Else = "else";
            IfElseThen = [If, Else, "then"]
    }

    #[test]
    fn keyword_bounded_iter() {
        assert_eq!(IfElseThen::keywords().bounded_len(2), 2);
        assert_eq!(IfElseThen::keywords().bounded_len(20), 3);
    }

    #[test]
    #[should_panic(expected = "not a valid keyword")]
    fn invalid_keyword() {
        keyword! {
            InvalidKeyword = [IfElseThen, "works", "000 is not a keyword"];
        }
        let mut tokens = "test".into_token_iter();
        let _: InvalidKeyword = tokens.parse().unwrap();
    }
}
