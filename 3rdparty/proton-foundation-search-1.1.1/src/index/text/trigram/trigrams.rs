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

#[test]
fn test_trigrams_french() {
    // French with common accented characters: é, è, ê, à, ù, ç, œ
    // Note: positions are byte offsets, not character offsets
    // Accented Latin characters (é, è, ê, ç, œ, etc.) are 2 bytes in UTF-8

    // "présentation" - common French word with accent
    // p(0) r(1) é(2-3) s(4) e(5) n(6) t(7) a(8) t(9) i(10) o(11) n(12)
    assert_eq!(
        "présentation".trigrams().collect::<Vec<_>>(),
        vec![
            (0, "pré"),  // starts at byte 0 (p)
            (1, "rés"),  // starts at byte 1 (r)
            (2, "ése"),  // starts at byte 2 (é)
            (4, "sen"),  // starts at byte 4 (s)
            (5, "ent"),  // starts at byte 5 (e)
            (6, "nta"),  // starts at byte 6 (n)
            (7, "tat"),  // starts at byte 7 (t)
            (8, "ati"),  // starts at byte 8 (a)
            (9, "tio"),  // starts at byte 9 (t)
            (10, "ion"), // starts at byte 10 (i)
        ]
    );

    // "café" - short word with accent at the end
    // c(0) a(1) f(2) é(3-4)
    assert_eq!(
        "café".trigrams().collect::<Vec<_>>(),
        vec![(0, "caf"), (1, "afé"),]
    );

    // "français" - with ç (c cedilla)
    // f(0) r(1) a(2) n(3) ç(4-5) a(6) i(7) s(8)
    assert_eq!(
        "français".trigrams().collect::<Vec<_>>(),
        vec![
            (0, "fra"),
            (1, "ran"),
            (2, "anç"),
            (3, "nça"),
            (4, "çai"), // ç starts at byte 4
            (6, "ais"), // a starts at byte 6 (after 2-byte ç)
        ]
    );

    // "cœur" - with œ ligature (2 bytes)
    // c(0) œ(1-2) u(3) r(4)
    assert_eq!(
        "cœur".trigrams().collect::<Vec<_>>(),
        vec![
            (0, "cœu"), // c(1) + œ(2) + u(1)
            (1, "œur"), // œ starts at byte 1
        ]
    );

    // "où" - very short, should return the whole string
    assert_eq!("où".trigrams().collect::<Vec<_>>(), vec![(0, "où")]);

    // "être" - with ê (e circumflex)
    // ê(0-1) t(2) r(3) e(4)
    assert_eq!(
        "être".trigrams().collect::<Vec<_>>(),
        vec![
            (0, "êtr"), // ê(2) + t(1) + r(1)
            (2, "tre"), // t starts at byte 2
        ]
    );
}

#[test]
fn test_trigrams_italian() {
    // Italian with accented vowels: à, è, é, ì, ò, ù

    // "città" - city, with à
    assert_eq!(
        "città".trigrams().collect::<Vec<_>>(),
        vec![
            (0, "cit"),
            (1, "itt"),
            (2, "ttà"), // à is 2 bytes
        ]
    );

    // "perché" - because/why, with é
    assert_eq!(
        "perché".trigrams().collect::<Vec<_>>(),
        vec![
            (0, "per"),
            (1, "erc"),
            (2, "rch"),
            (3, "ché"), // é is 2 bytes
        ]
    );

    // "lunedì" - Monday, with ì
    assert_eq!(
        "lunedì".trigrams().collect::<Vec<_>>(),
        vec![
            (0, "lun"),
            (1, "une"),
            (2, "ned"),
            (3, "edì"), // ì is 2 bytes
        ]
    );

    // "però" - however, with ò
    assert_eq!(
        "però".trigrams().collect::<Vec<_>>(),
        vec![
            (0, "per"),
            (1, "erò"), // ò is 2 bytes
        ]
    );

    // "più" - more, with ù (short word)
    assert_eq!(
        "più".trigrams().collect::<Vec<_>>(),
        vec![(0, "più")] // only 3 chars, returns as single trigram
    );

    // "università" - university, multiple accents possible context
    assert_eq!(
        "università".trigrams().collect::<Vec<_>>(),
        vec![
            (0, "uni"),
            (1, "niv"),
            (2, "ive"),
            (3, "ver"),
            (4, "ers"),
            (5, "rsi"),
            (6, "sit"),
            (7, "ità"), // à is 2 bytes
        ]
    );
}

#[test]
fn test_trigrams_french_phrases() {
    // Test a full French phrase: "l'élève étudie à l'école"
    // Words with spaces are tokenized separately in real usage,
    // but trigrams work on continuous strings
    // Note: positions are byte offsets

    // "élève" - student
    // é(0-1) l(2) è(3-4) v(5) e(6)
    assert_eq!(
        "élève".trigrams().collect::<Vec<_>>(),
        vec![
            (0, "élè"), // starts at byte 0 (é)
            (2, "lèv"), // starts at byte 2 (l)
            (3, "ève"), // starts at byte 3 (è)
        ]
    );

    // "étudie" - studies
    // é(0-1) t(2) u(3) d(4) i(5) e(6)
    assert_eq!(
        "étudie".trigrams().collect::<Vec<_>>(),
        vec![
            (0, "étu"), // starts at byte 0 (é)
            (2, "tud"), // starts at byte 2 (t)
            (3, "udi"), // starts at byte 3 (u)
            (4, "die"), // starts at byte 4 (d)
        ]
    );

    // "école" - school
    // é(0-1) c(2) o(3) l(4) e(5)
    assert_eq!(
        "école".trigrams().collect::<Vec<_>>(),
        vec![
            (0, "éco"), // starts at byte 0 (é)
            (2, "col"), // starts at byte 2 (c)
            (3, "ole"), // starts at byte 3 (o)
        ]
    );
}

#[test]
fn test_trigrams_italian_phrases() {
    // Test Italian words commonly used in phrases

    // "grazie" - thank you (no accents, baseline)
    assert_eq!(
        "grazie".trigrams().collect::<Vec<_>>(),
        vec![(0, "gra"), (1, "raz"), (2, "azi"), (3, "zie"),]
    );

    // "così" - so/thus, with ì
    assert_eq!(
        "così".trigrams().collect::<Vec<_>>(),
        vec![(0, "cos"), (1, "osì"),]
    );

    // "virtù" - virtue, with ù
    assert_eq!(
        "virtù".trigrams().collect::<Vec<_>>(),
        vec![(0, "vir"), (1, "irt"), (2, "rtù"),]
    );

    // "caffè" - coffee, with è (different from French café)
    assert_eq!(
        "caffè".trigrams().collect::<Vec<_>>(),
        vec![(0, "caf"), (1, "aff"), (2, "ffè"),]
    );
}

#[test]
fn test_trigrams_japanese() {
    // Japanese characters (hiragana, katakana, kanji) are each single Unicode code points
    // Each Japanese character is 3 bytes in UTF-8
    // This proves grapheme segmentation is not needed for standard Japanese text

    // "東京都" - Tokyo Metropolis (3 kanji, exactly one trigram)
    // 東(0-2) 京(3-5) 都(6-8)
    assert_eq!("東京都".trigrams().collect::<Vec<_>>(), vec![(0, "東京都")]);

    // "日本語" - Japanese language (3 kanji)
    assert_eq!("日本語".trigrams().collect::<Vec<_>>(), vec![(0, "日本語")]);

    // "東京都庁" - Tokyo Metropolitan Government (4 kanji)
    // 東(0-2) 京(3-5) 都(6-8) 庁(9-11)
    assert_eq!(
        "東京都庁".trigrams().collect::<Vec<_>>(),
        vec![(0, "東京都"), (3, "京都庁"),]
    );

    // "ありがとう" - thank you (5 hiragana)
    // あ(0-2) り(3-5) が(6-8) と(9-11) う(12-14)
    assert_eq!(
        "ありがとう".trigrams().collect::<Vec<_>>(),
        vec![(0, "ありが"), (3, "りがと"), (6, "がとう"),]
    );

    // "カタカナ" - katakana (4 katakana characters)
    // カ(0-2) タ(3-5) カ(6-8) ナ(9-11)
    assert_eq!(
        "カタカナ".trigrams().collect::<Vec<_>>(),
        vec![(0, "カタカ"), (3, "タカナ"),]
    );

    // "寿司" - sushi (2 kanji, less than trigram size, returns whole string)
    assert_eq!("寿司".trigrams().collect::<Vec<_>>(), vec![(0, "寿司")]);

    // Mixed: "東京Tower" - Tokyo Tower (kanji + ASCII)
    // 東(0-2) 京(3-5) T(6) o(7) w(8) e(9) r(10)
    assert_eq!(
        "東京Tower".trigrams().collect::<Vec<_>>(),
        vec![
            (0, "東京T"),
            (3, "京To"),
            (6, "Tow"),
            (7, "owe"),
            (8, "wer"),
        ]
    );
}
