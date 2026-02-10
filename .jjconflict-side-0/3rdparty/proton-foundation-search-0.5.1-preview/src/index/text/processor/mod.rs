//! Index processors.

use crate::index::text::processor::tokenizer::Tokenizer;

mod tokenizer;

/// Configuration for the text processor.
#[derive(Debug, Default)]
pub struct TextProcessorConfig {
    /// Minimum length of words to keep.
    pub min_length: Option<usize>,
    /// Maximum length of words to keep.
    pub max_length: Option<usize>,
    /// Should emojis be processed?
    pub enable_emojis: Option<bool>,
}

impl TextProcessorConfig {
    /// Sets the minimum word length.
    pub fn set_min_length(&mut self, min_length: usize) {
        self.min_length = Some(min_length);
    }

    /// Sets the minimum word length, returning `self`.
    pub fn with_min_length(mut self, min_length: usize) -> Self {
        self.set_min_length(min_length);
        self
    }

    /// Sets the maximum word length.
    pub fn set_max_length(&mut self, max_length: usize) {
        self.max_length = Some(max_length);
    }

    /// Sets the maximum word length, returning `self`.
    pub fn with_max_length(mut self, max_length: usize) -> Self {
        self.set_max_length(max_length);
        self
    }

    /// Sets emoji support.
    pub fn set_emojis(&mut self, enabled: bool) {
        self.enable_emojis = Some(enabled);
    }

    /// Sets emoji support, returning `self`.
    pub fn with_emojis(mut self, enabled: bool) -> Self {
        self.set_emojis(enabled);
        self
    }

    /// Builds the processor.
    pub fn build(&self) -> TextProcessor {
        TextProcessor {
            min_length: self.min_length.unwrap_or(3),
            max_length: self.max_length.unwrap_or(20),
            enable_emojis: self.enable_emojis.unwrap_or(true),
        }
    }
}

/// Processor for sanitizing the text values into values that can be indexed.
///
/// The input value will be split by words, only the words between the min and max length
/// will be kept and they will then be stemmed depending on the configure language.
///
/// # Example
///
/// ```rust
/// use proton_foundation_search::index::text::processor::*;
///
/// let processor: TextProcessor = TextProcessorConfig::default()
///     .with_min_length(5) // default value being 3
///     .with_max_length(42) // default value being 20
///     .build();
/// ```
pub struct TextProcessor {
    min_length: usize,
    max_length: usize,
    enable_emojis: bool,
}

impl std::fmt::Debug for TextProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Processor")
            .field("min_length", &self.min_length)
            .field("max_length", &self.max_length)
            .finish()
    }
}

impl TextProcessor {
    /// processes a text into smaller sanitized tokens
    ///
    /// The input value will be split by words, only the words between the min and max length
    /// will be kept and they will then be stemmed depending on the configure language.
    ///
    /// The process operation can have its configuration overridden, for the language,
    /// for example by passing a [TextProcessorConfig] as second parameter.
    #[tracing::instrument(skip_all)]
    pub fn process(&self, input: &str) -> Vec<(usize, Box<str>)> {
        Tokenizer::new(input, self.min_length, self.max_length, self.enable_emojis)
            .map(move |(pos, token)| (pos, token.to_lowercase().into_boxed_str()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use test_log::test;

    use super::*;

    #[test]
    fn should_build_processor() {
        let proc: TextProcessor = TextProcessorConfig::default().build();
        assert_eq!(proc.min_length, 3);
        assert_eq!(proc.max_length, 20);

        let proc: TextProcessor = TextProcessorConfig::default()
            .with_min_length(5)
            .with_max_length(10)
            .build();
        assert_eq!(proc.min_length, 5);
        assert_eq!(proc.max_length, 10);
    }

    #[test]
    fn should_process_text() {
        let proc = TextProcessor {
            min_length: 3,
            max_length: 20,
            enable_emojis: true,
        };
        let res = proc.process("Hello World with SmAll Words");
        // plural because we don't use stemming/inflectors
        insta::assert_debug_snapshot!(
            res, @r#"
        [
            (
                0,
                "hello",
            ),
            (
                6,
                "world",
            ),
            (
                12,
                "with",
            ),
            (
                17,
                "small",
            ),
            (
                23,
                "words",
            ),
        ]
        "#
        );

        let res = proc
            .process("This word will be too long pneumonoultramicroscopicsilicovolcanoconiosis");
        insta::assert_debug_snapshot!(
            res, @r#"
        [
            (
                0,
                "this",
            ),
            (
                5,
                "word",
            ),
            (
                10,
                "will",
            ),
            (
                18,
                "too",
            ),
            (
                22,
                "long",
            ),
        ]
        "#
        );
    }

    #[test]
    fn should_process_text_with_edge_lengths() {
        let proc = TextProcessor {
            min_length: 3,
            max_length: 5,
            enable_emojis: true,
        };
        let res = proc.process("ab abc abcd abcde abcdef");
        insta::assert_debug_snapshot!(
            res, @r#"
        [
            (
                3,
                "abc",
            ),
            (
                7,
                "abcd",
            ),
            (
                12,
                "abcde",
            ),
        ]
        "#
        );
    }

    #[test]
    fn should_process_empty_text() {
        let proc = TextProcessor {
            min_length: 3,
            max_length: 20,
            enable_emojis: true,
        };
        let res = proc.process("");
        assert!(res.is_empty());
    }

    #[test]
    fn should_process_text_with_no_valid_words() {
        let proc = TextProcessor {
            min_length: 3,
            max_length: 20,
            enable_emojis: true,
        };
        let res = proc.process("  !@#$%^&*()_+=-`~[]{}\\|;:'\",./<>?  ");
        assert_eq!(res, vec![]);

        let res = proc.process("a bb ccc dddd !@#$%^&*");
        insta::assert_debug_snapshot!(
            res, @r#"
        [
            (
                5,
                "ccc",
            ),
            (
                9,
                "dddd",
            ),
        ]
        "#);
    }

    #[test]
    fn should_process_hyphens() {
        let proc = TextProcessor {
            min_length: 3,
            max_length: 20,
            enable_emojis: true,
        };
        let res = proc.process("Fromage dans grands-mères");
        insta::assert_debug_snapshot!(
            res, @r#"
        [
            (
                0,
                "fromage",
            ),
            (
                8,
                "dans",
            ),
            (
                13,
                "grands",
            ),
            (
                20,
                "mères",
            ),
        ]
        "#);
    }
}
