use crate::test::matcher::Matcher;

/// An 'all-of' matcher:
/// applies a matcher to all elements of the given slice,
/// returning true if all elements match.
#[derive(Debug)]
pub struct All<M>(M);

/// Create a new 'all-of' matcher.
pub const fn all<M>(m: M) -> All<M> {
    All(m)
}

impl<T, M> Matcher<[T]> for All<M>
where
    M: Matcher<T>,
{
    fn matches(&self, val: &[T]) -> bool {
        val.iter().all(|val| self.0.matches(val))
    }
}

/// An 'any-of' matcher:
/// applies a matcher to all elements of the given slice,
/// returning true if any element matches.
#[derive(Debug)]
pub struct Any<M>(M);

/// Create a new 'any-of' matcher.
pub const fn any<M>(m: M) -> Any<M> {
    Any(m)
}

impl<T, M> Matcher<[T]> for Any<M>
where
    M: Matcher<T>,
{
    fn matches(&self, val: &[T]) -> bool {
        val.iter().any(|val| self.0.matches(val))
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    use crate::test::matcher::value::eq;

    #[test]
    fn test_match_all() {
        let m = all(eq(1));
        assert!(m.matches(&[1, 1, 1]));
        assert!(!m.matches(&[1, 2, 1]));
    }

    #[test]
    fn test_match_any() {
        let m = any(eq(1));
        assert!(m.matches(&[1, 2, 3]));
        assert!(!m.matches(&[2, 3, 4]));
    }
}
