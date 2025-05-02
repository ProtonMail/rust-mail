use super::*;
use std::fmt;
use std::ops::Deref;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    #[must_use]
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    #[must_use]
    pub fn resolve(&self, src: &[u8]) -> String {
        let (start_line, start_char) = Self::resolve_ex(src, self.start, true);
        let (end_line, end_char) = Self::resolve_ex(src, self.end, false);

        if start_line == end_line {
            if start_char == end_char {
                format!("{start_line}:{start_char}")
            } else {
                format!("{start_line}:{start_char}..{end_char}")
            }
        } else {
            format!("{start_line}:{start_char}..{end_line}:{end_char}")
        }
    }

    #[must_use]
    pub fn resolve_ex(src: &[u8], pos: usize, inclusive: bool) -> (usize, usize) {
        let mut line = 1;
        let mut char = 1;

        let pos = if inclusive { pos } else { pos - 1 };

        for &ch in src.iter().take(pos) {
            if ch == b'\r' || ch == b'\n' {
                line += 1;
                char = 1;
            } else {
                char += 1;
            }
        }

        (line, char)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spanned<T> {
    pub span: Span,
    pub value: T,
}

impl<T> Spanned<T> {
    #[must_use]
    pub fn new(span: Span, value: T) -> Self {
        Self { span, value }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Spanned<U> {
        Spanned {
            span: self.span,
            value: f(self.value),
        }
    }
}

impl<T, E> Spanned<Result<T, E>>
where
    E: fmt::Display,
{
    pub fn unwrap(self, r: &mut Reader) -> Option<T> {
        match self.value {
            Ok(value) => Some(value),

            Err(err) => {
                r.error(self.span, err);
                None
            }
        }
    }
}

impl<T> Deref for Spanned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T, M> Read<M> for Spanned<T>
where
    T: Read<M>,
{
    fn read(r: &mut Reader) -> Option<Self> {
        r.spanned(T::read)
    }

    fn name() -> String {
        T::name()
    }
}
