//! Conversions of input values to indexed values and matching conversion of query input

mod config;
mod tokenizer;

#[cfg(test)]
mod tests;

use std::collections::HashSet;

pub use config::ProcessorConfig;
use tokenizer::Tokenizer;

/// Trait for value processing
pub trait Processor: std::fmt::Debug + Send + Sync {
    /// Convert Document Values to Entry values
    fn process_document_tokens(&self, value: &str) -> Vec<(usize, Box<str>)>;

    /// Tokenize query text so it can be matched against IndexedValue text
    fn process_query_term(&self, term: &str) -> Vec<(usize, Box<str>)>;
}

/// Processor for sanitizing the text values into values that can be indexed.
///
/// The input value will be split by words, only the words between the min and max length
/// will be kept and they will then be stemmed depending on the configure language.
///
/// The tokens are converted to lower case.
///
/// # Example
///
/// ```rust
/// use proton_foundation_search::processor::*;
///
/// let processor: TextProcessor = ProcessorConfig::default()
///     .with_min_length(5) // default value being 3
///     .with_max_length(42) // default value being 20
///     .build();
/// ```
#[derive(Debug)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct TextProcessor {
    min_length: usize,
    max_length: usize,
    enable_emojis: bool,
    stop_words: HashSet<Box<str>>,
}

impl TextProcessor {
    fn process(&self, value: &str, query: bool) -> Vec<(usize, Box<str>)> {
        let min_length = if query { 0 } else { self.min_length };
        Tokenizer::new(value, min_length, self.max_length, self.enable_emojis)
            .map(|(pos, token)| (pos, token.to_lowercase()))
            .filter(|(_pos, token)| !self.stop_words.contains(token.as_str()))
            .map(|(pos, token)| (pos, token.into_boxed_str()))
            .collect()
    }
}

impl Processor for TextProcessor {
    /// processes a text into smaller sanitized tokens
    ///
    /// The input value will be split by words, only the words between the min and max length
    /// will be kept.
    ///
    /// The process operation can have its configuration overridden,
    /// for example by passing a [TextProcessorConfig] as second parameter.
    #[tracing::instrument]
    fn process_document_tokens(&self, value: &str) -> Vec<(usize, Box<str>)> {
        self.process(value, false)
    }

    /// processes a query term into smaller sanitized tokens
    #[tracing::instrument]
    fn process_query_term(&self, term: &str) -> Vec<(usize, Box<str>)> {
        self.process(term, true)
    }
}
