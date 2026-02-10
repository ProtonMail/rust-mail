use crate::test::matcher::Matcher;

/// An 'and' combinator: both the lhs and rhs must match.
#[derive(Debug)]
pub struct And<L, R>(L, R);

/// Create a new 'and' combinator.
pub const fn and<L, R>(l: L, r: R) -> And<L, R> {
    And(l, r)
}

/// Creates an 'and' combinator from a list of matchers.
#[macro_export]
macro_rules! and {
    ($head:expr, $($tail:expr),* $(,)?) => {
        $crate::test::matcher::combinator::and($head, and!($($tail),*))
    };

    ($head:expr) => {
        $head
    };
}

impl<T: ?Sized, L, R> Matcher<T> for And<L, R>
where
    L: Matcher<T>,
    R: Matcher<T>,
{
    fn matches(&self, val: &T) -> bool {
        self.0.matches(val) && self.1.matches(val)
    }
}

/// An 'or' combinator: either the lhs or rhs must match.
#[derive(Debug)]
pub struct Or<L, R>(L, R);

/// Create a new 'or' combinator.
pub const fn or<L, R>(l: L, r: R) -> Or<L, R> {
    Or(l, r)
}

/// Creates an 'or' combinator from a list of matchers.
#[macro_export]
macro_rules! or {
    ($head:expr, $($tail:expr),* $(,)?) => {
        $crate::test::matcher::combinator::or($head, or!($($tail),*))
    };

    ($head:expr) => {
        $head
    };
}

impl<T: ?Sized, L, R> Matcher<T> for Or<L, R>
where
    L: Matcher<T>,
    R: Matcher<T>,
{
    fn matches(&self, val: &T) -> bool {
        self.0.matches(val) || self.1.matches(val)
    }
}

/// A 'not' combinator: the inner matcher must not match.
#[derive(Debug)]
pub struct Not<M>(M);

/// Create a new 'not' combinator.
pub const fn not<M>(m: M) -> Not<M> {
    Not(m)
}

impl<T: ?Sized, M> Matcher<T> for Not<M>
where
    M: Matcher<T>,
{
    fn matches(&self, val: &T) -> bool {
        !self.0.matches(val)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test::matcher::value::{eq, re};

    #[test]
    fn test_match_and() {
        let m = and(eq("foo"), re(r".{3}"));
        assert!(m.matches("foo"));
        assert!(!m.matches("fo"));
        assert!(!m.matches("fooo"));
        assert!(!m.matches("bar"));
    }

    #[test]
    fn test_match_or() {
        let m = or(eq("foo"), eq("bar"));
        assert!(m.matches("foo"));
        assert!(m.matches("bar"));
        assert!(!m.matches("baz"));
    }

    #[test]
    fn test_match_not() {
        let m = not(eq("foo"));
        assert!(!m.matches("foo"));
        assert!(m.matches("bar"));
    }

    #[test]
    fn test_match_not_and() {
        let m = not(and(eq("foo"), re(r".{3}")));
        assert!(!m.matches("foo"));
        assert!(m.matches("fo"));
        assert!(m.matches("fooo"));
        assert!(m.matches("bar"));
    }

    #[test]
    fn test_match_not_or() {
        let m = not(or(eq("foo"), eq("bar")));
        assert!(!m.matches("foo"));
        assert!(!m.matches("bar"));
        assert!(m.matches("baz"));
    }

    #[test]
    fn test_match_not_not() {
        let m = not(not(eq("foo")));
        assert!(m.matches("foo"));
        assert!(!m.matches("bar"));
    }

    #[test]
    fn test_match_nested() {
        let m = and!(or!(eq("foo"), eq("bar"), eq("qux")), not(eq("baz")));
        assert!(m.matches("foo"));
        assert!(m.matches("bar"));
        assert!(m.matches("qux"));
        assert!(!m.matches("baz"));
    }
}
