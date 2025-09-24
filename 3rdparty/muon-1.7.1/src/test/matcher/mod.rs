use muon_proc::{autoimpl, derive_dyn};

export! {
    /// Matchers for single values.
    mod value (as pub);

    /// Matchers for slices.
    mod slice (as pub);

    /// Combinators for matchers.
    mod combinator (as pub);
}

/// Check if something is what we want.
#[derive_dyn(Debug)]
pub trait Matcher<T: ?Sized>: Send + Sync + 'static {
    /// Check if the given value matches.
    fn matches(&self, val: &T) -> bool;
}

/// A slice matcher: matches a slice of values of type `T`.
#[autoimpl]
#[derive_dyn(Debug)]
pub trait SliceMatcher<T: ?Sized>: for<'a> Matcher<[&'a T]> {}

/// A map matcher: matches a map of keys `K` and values `V`.
#[autoimpl]
#[derive_dyn(Debug)]
pub trait MapMatcher<K: ?Sized, V: ?Sized>: for<'a> Matcher<[(&'a K, &'a V)]> {}

/// A predicate matcher: a matcher that uses a predicate to match.
#[derive(Debug)]
pub struct Pred<M>(M);

/// Create a new predicate matcher.
pub const fn pred<M>(m: M) -> Pred<M> {
    Pred(m)
}

impl<T: ?Sized, M> Matcher<T> for Pred<M>
where
    M: Fn(&T) -> bool + Send + Sync + 'static,
{
    fn matches(&self, val: &T) -> bool {
        self.0(val)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_predicate() {
        let m = pred(|&x: &i32| x == 42);
        assert!(m.matches(&42));
        assert!(!m.matches(&0));

        let m = pred(|x: &str| x.is_empty());
        assert!(m.matches(""));
        assert!(!m.matches("hello"));

        let m = pred(|x: &str| x.len() == 3);
        assert!(m.matches("foo"));
        assert!(!m.matches("hello"));
    }
}
