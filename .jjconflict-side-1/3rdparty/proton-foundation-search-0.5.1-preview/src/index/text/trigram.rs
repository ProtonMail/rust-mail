//! Trigram extraction extension on stringy data

use std::iter::once;

/// Extension for extracting trigrams from strings
pub trait Trigrams {
    /// Extract trigrams from strings together with position
    fn trigrams(&self) -> impl Iterator<Item = (usize, &str)>;
}

impl<T: ?Sized + AsRef<str>> Trigrams for T {
    fn trigrams(&self) -> impl Iterator<Item = (usize, &str)> {
        OrIter::new(CharWindows::new(self.as_ref(), 2), once((0, self.as_ref())))
    }
}

struct OrIter<I, O> {
    iter: I,
    or: O,
    some: bool,
}

impl<I, O> OrIter<I, O> {
    fn new(iter: I, or: O) -> Self {
        Self {
            iter,
            or,
            some: false,
        }
    }
}
impl<I: Iterator, O: Iterator> Iterator for OrIter<I, O>
where
    O::Item: Into<I::Item>,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.iter.next();
        match next {
            Some(x) => {
                self.some = true;
                Some(x)
            }
            None if self.some => None,
            None => self.or.next().map(|x| x.into()),
        }
    }
}

struct CharWindows<'a> {
    position: usize,
    extras: usize,
    input: &'a str,
}

impl<'a> CharWindows<'a> {
    /// * extras: how many extra characters to iterate over, 0 means only 1 char at a time, 1 means 2 characters at a time
    fn new(input: &'a str, extras: usize) -> Self {
        Self {
            position: 0,
            extras,
            input,
        }
    }
}

impl<'a> Iterator for CharWindows<'a> {
    type Item = (usize, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        let Self {
            position,
            extras,
            input,
        } = self;
        let pos = *position;
        let (trigram, next) = {
            let mut boundaries = input.char_bundaries();

            let first = boundaries.next()?;
            assert_eq!(first, 0, "correctly formed string will start at a boundary");

            let mut second = None;
            for i in 0..*extras {
                let next = boundaries.next()?;
                if i == 0 {
                    second = Some(next);
                }
            }
            let end = boundaries.next().unwrap_or(input.len());
            let second = second.unwrap_or(end);
            *position += second;
            (&input[0..end], second)
        };
        *input = &input[next..];
        Some((pos, trigram))
    }
}

/// Extensions on any str-ingy type
pub trait Chars: AsRef<str> {
    /// Split at the nth char.
    /// If the pos is out of bounds, the first part will have all chars and the second will be empty.
    fn split_chars(&self, pos: usize) -> (&str, &str) {
        split_chars(self.as_ref(), pos)
    }

    /// get the number of bytes the first n chars occuppy
    /// if the str is shorter, the full length is returned
    fn char_length(&self, n: usize) -> usize {
        self.char_bundaries()
            .nth(n)
            .unwrap_or_else(|| self.as_ref().len())
    }

    /// get the number of characters in the string
    fn count_chars(&self) -> usize {
        self.char_bundaries().count()
    }

    /// iterate over character boundaries
    fn char_bundaries(&self) -> impl DoubleEndedIterator<Item = usize> {
        let data = self.as_ref();
        data.bytes()
            .enumerate()
            .filter_map(|(pos, b)| is_utf8_boundary(b).then_some(pos))
    }
}
impl<T: AsRef<str> + ?Sized> Chars for T {}

/// Split at the nth char
/// If the pos is out of bounds, the first part will have all chars and the second will be empty.
/// For cases we cannot use the trait method because of ownership issues
fn split_chars(data: &str, pos: usize) -> (&str, &str) {
    let pos = data.char_length(pos);
    (&data[0..pos], &data[pos..])
}

/// check if the byte is a UTF-8 boundary byte
fn is_utf8_boundary(b: u8) -> bool {
    b & 0xC0 != 0x80
}

#[test]
fn test_is_utf8_boundary() {
    let boundaries = "o -\0ěščřž\n"
        .bytes()
        .enumerate()
        .filter_map(|(pos, b)| is_utf8_boundary(b).then_some(pos))
        .collect::<Vec<_>>();
    assert_eq!(boundaries, vec![0, 1, 2, 3, 4, 6, 8, 10, 12, 14]);
}

#[test]
fn test_char_bundaries() {
    let boundaries = "o -\0ěščřž\n".char_bundaries().collect::<Vec<_>>();
    assert_eq!(boundaries, vec![0, 1, 2, 3, 4, 6, 8, 10, 12, 14]);
}

#[test]
fn test_take_chars() {
    let (head, tail) = "o -\0ěščřž".split_chars(6);
    assert_eq!((head, tail), ("o -\0ěš", "čřž"));
}

#[test]
fn test_char_iter_ascii() {
    let mut sut = CharWindows::new("glory", 0);
    assert_eq!(sut.next(), Some((0, "g")));
    assert_eq!(sut.position, 1);
    assert_eq!(sut.input, "lory");
    assert_eq!(sut.next(), Some((1, "l")));
    assert_eq!(sut.position, 2);
    assert_eq!(sut.input, "ory");
    assert_eq!(sut.next(), Some((2, "o")));
    assert_eq!(sut.position, 3);
    assert_eq!(sut.input, "ry");
    assert_eq!(sut.next(), Some((3, "r")));
    assert_eq!(sut.position, 4);
    assert_eq!(sut.input, "y");
    assert_eq!(sut.next(), Some((4, "y")));
    assert_eq!(sut.position, 5);
    assert_eq!(sut.input, "");
    assert_eq!(sut.next(), None);
    assert_eq!(sut.position, 5);
    assert_eq!(sut.input, "");

    assert_eq!(
        CharWindows::new("glory", 0).collect::<Vec<_>>(),
        vec![(0, "g"), (1, "l"), (2, "o"), (3, "r"), (4, "y")]
    );
    assert_eq!(
        CharWindows::new("glory", 1).collect::<Vec<_>>(),
        vec![(0, "gl"), (1, "lo"), (2, "or"), (3, "ry")]
    );
    assert_eq!(
        CharWindows::new("glory", 2).collect::<Vec<_>>(),
        vec![(0, "glo"), (1, "lor"), (2, "ory")]
    );
}

#[test]
fn test_char_iter_multibyte() {
    assert_eq!(
        CharWindows::new("வரவேற்பு", 0).collect::<Vec<_>>(),
        vec![
            (0, "வ"),
            (3, "ர"),
            (6, "வ"),
            (9, "ே"),
            (12, "ற"),
            (15, "\u{bcd}"),
            (18, "ப"),
            (21, "ு")
        ]
    );
    assert_eq!(
        CharWindows::new("வரவேற்பு", 1).collect::<Vec<_>>(),
        vec![
            (0, "வர"),
            (3, "ரவ"),
            (6, "வே"),
            (9, "ேற"),
            (12, "ற\u{bcd}"),
            (15, "\u{bcd}ப"),
            (18, "பு")
        ]
    );
    assert_eq!(
        CharWindows::new("வரவேற்பு", 2).collect::<Vec<_>>(),
        vec![
            (0, "வரவ"),
            (3, "ரவே"),
            (6, "வேற"),
            (9, "ேற\u{bcd}"),
            (12, "ற\u{bcd}ப"),
            (15, "\u{bcd}பு")
        ]
    );
}

#[test]
fn test_trigrams() {
    assert_eq!(
        "abcdefg".trigrams().collect::<Vec<_>>(),
        vec![(0, "abc"), (1, "bcd"), (2, "cde"), (3, "def"), (4, "efg")]
    );

    assert_eq!(
        "ěščxyžř".trigrams().collect::<Vec<_>>(),
        vec![(0, "ěšč"), (2, "ščx"), (4, "čxy"), (6, "xyž"), (7, "yžř")]
    );

    assert_eq!(
        "வரவேற்பு".trigrams().collect::<Vec<_>>(),
        vec![
            (0, "வரவ"),
            (3, "ரவே"),
            (6, "வேற"),
            (9, "ேற\u{bcd}"),
            (12, "ற\u{bcd}ப"),
            (15, "\u{bcd}பு")
        ]
    );

    assert_eq!(
        "👌💕🧚🏻‍♀️✨🍄🍃".trigrams().collect::<Vec<_>>(),
        vec![
            (0, "👌💕🧚"),
            (4, "💕🧚🏻"),
            (8, "🧚🏻\u{200d}"),
            (12, "🏻\u{200d}♀"),
            (16, "\u{200d}♀\u{fe0f}"),
            (19, "♀\u{fe0f}✨"),
            (22, "\u{fe0f}✨🍄"),
            (25, "✨🍄🍃"),
        ]
    );
}
