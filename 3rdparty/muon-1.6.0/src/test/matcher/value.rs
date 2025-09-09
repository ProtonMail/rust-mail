use crate::test::matcher::Matcher;
use regex::Regex;
use std::borrow::Borrow;
use std::fmt::Debug;

/// An exact matcher.
#[derive(Debug)]
pub struct Eq<M>(M);

/// Create a new exact matcher.
pub const fn eq<M>(val: M) -> Eq<M> {
    Eq(val)
}

impl<T: ?Sized, M> Matcher<T> for Eq<M>
where
    T: PartialEq,
    M: Borrow<T> + Send + Sync + 'static,
{
    fn matches(&self, val: &T) -> bool {
        val == self.0.borrow()
    }
}

/// A regex matcher.
#[derive(Debug)]
pub struct Re(Regex);

/// Create a new regex matcher.
///
/// # Panics
///
/// Panics if the regex pattern is invalid.
#[must_use]
pub fn re(val: &str) -> Re {
    Re(Regex::new(val).unwrap())
}

impl<T: ?Sized> Matcher<T> for Re
where
    T: Borrow<str>,
{
    fn matches(&self, val: &T) -> bool {
        self.0.is_match(val.borrow())
    }
}

/// A pair matcher, matching a 2-tuple.
///
/// This matcher matches tuples where the first element matches the left matcher
/// and the second element matches the right matcher.
#[derive(Debug)]
pub struct Pair<L, R>(L, R);

/// Create a new pair matcher.
pub const fn pair<K, V>(l: K, r: V) -> Pair<K, V> {
    Pair(l, r)
}

impl<K: ?Sized, V: ?Sized, LM, RM> Matcher<(&K, &V)> for Pair<LM, RM>
where
    LM: Matcher<K>,
    RM: Matcher<V>,
{
    fn matches(&self, val: &(&K, &V)) -> bool {
        self.0.matches(val.0) && self.1.matches(val.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_eq_int() {
        let m = eq(1);
        assert!(m.matches(&1));
        assert!(!m.matches(&2));
    }

    #[test]
    fn test_match_eq_str() {
        let m = eq("foo");
        assert!(m.matches("foo"));
        assert!(!m.matches("bar"));
    }

    #[test]
    fn test_match_array() {
        let m = eq([1, 2, 3]);
        assert!(m.matches(&[1, 2, 3]));
        assert!(!m.matches(&[1, 2, 4]));
    }

    #[test]
    fn test_match_re() {
        let m = re(r"\d+");
        assert!(m.matches("123"));
        assert!(!m.matches("abc"));
    }

    #[test]
    fn test_match_pair() {
        let m = pair(eq(1), re(r"\d+"));
        assert!(m.matches(&(&1, "123")));
        assert!(!m.matches(&(&2, "123")));
        assert!(!m.matches(&(&1, "abc")));
    }
}
