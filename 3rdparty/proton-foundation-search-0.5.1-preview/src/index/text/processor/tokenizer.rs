use std::collections::VecDeque;
use std::str::CharIndices;

use tracing::instrument;

pub struct Tokenizer<'a> {
    min_word_length: usize,
    max_word_length: usize,
    enable_emojis: bool,
    mode: Mode,
    start: usize,
    input: &'a str,
    indices: CharIndices<'a>,
    tokens: VecDeque<(usize, &'a str)>,
}

impl<'a> Tokenizer<'a> {
    pub fn new(
        input: &'a str,
        min_word_length: usize,
        max_word_length: usize,
        enable_emojis: bool,
    ) -> Self {
        Self {
            min_word_length,
            max_word_length,
            enable_emojis,
            mode: Mode::None,
            start: 0,
            tokens: [].into(),
            indices: input.char_indices(),
            input,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    None,
    Word,
    Emoji,
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = (usize, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(token) = self.tokens.pop_front() {
                break Some(token);
            }

            let (pos, next) = self.indices.next()?;

            let (break_new, next_mode) = Self::check_next(&self.input[self.start..pos], next);
            if break_new || self.mode != next_mode {
                // change of mode or break: produce a token, shift start, switch mode
                self.matched(pos, next_mode);
            }

            let is_last = pos + next.len_utf8() == self.input.len();
            if is_last {
                // this is the end of the input and we may have a token
                self.matched(self.input.len(), Mode::None);
            }
        }
    }
}

impl<'a> Tokenizer<'a> {
    /// Update state, add token if applicable
    fn matched(&mut self, pos: usize, next_mode: Mode) {
        let token = &self.input[self.start..pos];
        let add_token = match self.mode {
            Mode::None => false,
            Mode::Word => (self.min_word_length..=self.max_word_length)
                .contains(&token.char_indices().count()),
            Mode::Emoji => self.enable_emojis,
        };
        if add_token {
            self.tokens.push_back((self.start, token));
        }
        self.start = pos;
        self.mode = next_mode;
    }

    /// Determine the next mode and whether to break
    #[instrument]
    fn check_next(sequence: &str, next: char) -> (bool, Mode) {
        if next.is_alphanumeric() {
            return (false, Mode::Word);
        }

        let is_emoji = Self::is_emoji(sequence, next);

        // tracing::trace!(
        //     ?is_emoji,
        //     emoji = unic_emoji::char::is_emoji(next),
        //     component = unic_emoji::char::is_emoji_component(next),
        //     modifier = unic_emoji::char::is_emoji_modifier(next),
        //     modifier_base = unic_emoji::char::is_emoji_modifier_base(next),
        //     presentation = unic_emoji::char::is_emoji_presentation(next),
        // );

        if let Some(break_new) = is_emoji {
            (break_new, Mode::Emoji)
        } else {
            (false, Mode::None)
        }
    }

    /// Some() = if the sequence so far and the next char together is an actual emoji symbol
    /// true = if a new emoji sequence is starting - break new
    fn is_emoji(sequence: &str, next: char) -> Option<bool> {
        // TODO: check sequence validity
        //   - max length as a sanity check
        //   - only some emoji char combinations may be correct
        // TODO: make unic_emoji dependency optional with a compilation feature flag?
        // CHECKME: based on experimental reverse-engineering of the unic_emoji

        let joiner = '\u{200D}'; //ZWJ - zero width joiner

        let presentation = [
            '\u{FE0E}', //Presentation text
            '\u{FE0F}', //Presentation image
        ];

        // unic_emoji::char::is_emoji gives true for '*' or '#'
        // which are valid only following an initial emoji char
        let is_emoji_initial =
            unic_emoji::char::is_emoji(next) && !unic_emoji::char::is_emoji_component(next);

        if sequence.is_empty() {
            // initial emoji char?
            is_emoji_initial.then_some(false)
        } else if sequence.ends_with(joiner) {
            // following a joiner
            is_emoji_initial.then_some(false)
        } else if next == joiner {
            // joiner following non-empty sequence
            Some(false)
        } else if unic_emoji::char::is_emoji_modifier(next) || presentation.contains(&next) {
            // emoji modifier or presentation
            Some(false)
        } else {
            // new emoji starting? break new
            is_emoji_initial.then_some(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use test_log::test;

    use super::*;

    #[test]
    fn tokenize_ascii_text() {
        insta::assert_debug_snapshot!(Tokenizer::new(" world hello's... ",0,20,true).collect::<Vec<_>>(), @r#"
    [
        (
            1,
            "world",
        ),
        (
            7,
            "hello",
        ),
        (
            13,
            "s",
        ),
    ]
    "#);
    }

    #[test]
    fn tokenize_unicode_text() {
        insta::assert_debug_snapshot!(Tokenizer::new("máváme žlabavě",0,20,true).collect::<Vec<_>>(), @r#"
    [
        (
            0,
            "máváme",
        ),
        (
            9,
            "žlabavě",
        ),
    ]
    "#);
    }

    #[test]
    fn tokenize_alnum() {
        // ref https://machs.space/posts/whats-the-max-valid-length-of-an-emoji/
        insta::assert_debug_snapshot!(Tokenizer::new("amd64 with 64bit arch ",0,20,true).collect::<Vec<_>>(), @r#"
        [
            (
                0,
                "amd64",
            ),
            (
                6,
                "with",
            ),
            (
                11,
                "64bit",
            ),
            (
                17,
                "arch",
            ),
        ]
        "#);
    }

    #[test]
    fn tokenize_emoji_with_text() {
        // ref https://machs.space/posts/whats-the-max-valid-length-of-an-emoji/
        insta::assert_debug_snapshot!(Tokenizer::new("cool😎🤓that o👨🏻‍❤️‍💋‍👨🏻o",0,20,true).collect::<Vec<_>>(), @r#"
        [
            (
                0,
                "cool",
            ),
            (
                4,
                "😎",
            ),
            (
                8,
                "🤓",
            ),
            (
                12,
                "that",
            ),
            (
                17,
                "o",
            ),
            (
                18,
                "👨🏻\u{200d}❤\u{fe0f}\u{200d}💋\u{200d}👨🏻",
            ),
            (
                53,
                "o",
            ),
        ]
        "#);
    }

    #[test]
    fn tokenize_empty_text() {
        insta::assert_debug_snapshot!(Tokenizer::new("",0,20,true).collect::<Vec<_>>(), @"[]");
        insta::assert_debug_snapshot!(Tokenizer::new("    ",0,20,true).collect::<Vec<_>>(), @"[]");
    }

    #[test]
    fn tokenize_word_limits() {
        // checking limit lengths of words
        insta::assert_compact_debug_snapshot!(Tokenizer::new("a",0,1,true).collect::<Vec<_>>(), @"[(0, \"a\")]");
        insta::assert_compact_debug_snapshot!(Tokenizer::new("a",3,20,true).collect::<Vec<_>>(), @"[]");
        insta::assert_compact_debug_snapshot!(Tokenizer::new("abcd",2,3,true).collect::<Vec<_>>(), @"[]");
        // multi-byte chars
        insta::assert_compact_debug_snapshot!(Tokenizer::new("š",1,1,true).collect::<Vec<_>>(), @"[(0, \"š\")]");
    }

    #[test]
    fn tokenize_words_disabled_emojis() {
        // emojis disabled - only alnum
        insta::assert_compact_debug_snapshot!(Tokenizer::new("💋es",0,20,false).collect::<Vec<_>>(), @"[(4, \"es\")]");
        // same but enabled - includes the emoji
        insta::assert_compact_debug_snapshot!(Tokenizer::new("💋es",0,20,true).collect::<Vec<_>>(), @"[(0, \"💋\"), (4, \"es\")]");
    }

    #[test]
    fn tokenize_length_limits_ignored_on_emoji() {
        // word length limits do not apply to emojis
        insta::assert_debug_snapshot!(Tokenizer::new("👨🏻‍❤️‍💋‍👨🏻",0,1,true).collect::<Vec<_>>(), @r#"
        [
            (
                0,
                "👨🏻\u{200d}❤\u{fe0f}\u{200d}💋\u{200d}👨🏻",
            ),
        ]
        "#);
        insta::assert_debug_snapshot!(Tokenizer::new("👨🏻‍❤️‍💋‍👨🏻",100,1000,true).collect::<Vec<_>>(), @r#"
        [
            (
                0,
                "👨🏻\u{200d}❤\u{fe0f}\u{200d}💋\u{200d}👨🏻",
            ),
        ]
        "#);
    }
}
