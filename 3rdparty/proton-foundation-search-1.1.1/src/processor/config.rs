use super::*;

/// Configuration for the text processor.
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct ProcessorConfig {
    /// Minimum length of words to keep.
    pub min_length: Option<usize>,
    /// Maximum length of words to keep.
    pub max_length: Option<usize>,
    /// Should emojis be processed?
    pub enable_emojis: Option<bool>,
    /// Stop words to skip
    #[cfg(feature = "wasm-bindgen")]
    #[wasm_bindgen(skip)]
    pub stop_words: Vec<Box<str>>,
    #[cfg(not(feature = "wasm-bindgen"))]
    pub stop_words: Vec<Box<str>>,
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl ProcessorConfig {
    /// Sets the minimum word length.
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "setMinLength")
    )]
    pub fn set_min_length(&mut self, min_length: usize) {
        self.min_length = Some(min_length);
    }

    /// Sets the minimum word length, returning `self`.
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "withMinLength")
    )]
    pub fn with_min_length(mut self, min_length: usize) -> Self {
        self.set_min_length(min_length);
        self
    }

    /// Sets the maximum word length.
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "setMaxLength")
    )]
    pub fn set_max_length(&mut self, max_length: usize) {
        self.max_length = Some(max_length);
    }

    /// Sets the maximum word length, returning `self`.
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "withMaxLength")
    )]
    pub fn with_max_length(mut self, max_length: usize) -> Self {
        self.set_max_length(max_length);
        self
    }

    /// Sets emoji support.
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "setEmojis")
    )]
    pub fn set_emojis(&mut self, enabled: bool) {
        self.enable_emojis = Some(enabled);
    }

    /// Sets emoji support, returning `self`.
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "withEmojis")
    )]
    pub fn with_emojis(mut self, enabled: bool) -> Self {
        self.set_emojis(enabled);
        self
    }

    /// Add a stop word, returning `self`.
    #[cfg(feature = "wasm-bindgen")]
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = "addStopWord")]
    pub fn add_stop_word_wasm(&mut self, stop_word: String) {
        self.add_stop_word(stop_word);
    }

    /// Add a stop word, returning `self`.
    #[cfg(feature = "wasm-bindgen")]
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = "withStopWord")]
    pub fn with_stop_word_wasm(self, stop_word: String) -> Self {
        self.with_stop_word(stop_word)
    }

    /// Builds the processor.
    pub fn build(&self) -> TextProcessor {
        TextProcessor::new(self)
    }
}

impl ProcessorConfig {
    /// Create a new constructor config with given values
    pub fn new(
        min_length: Option<usize>,
        max_length: Option<usize>,
        enable_emojis: Option<bool>,
        stop_words: Vec<Box<str>>,
    ) -> Self {
        Self {
            min_length,
            max_length,
            enable_emojis,
            stop_words,
        }
    }

    /// Add a stop word.
    pub fn add_stop_word(&mut self, stop_word: impl Into<Box<str>>) {
        self.stop_words.push(stop_word.into());
    }

    /// Add a stop word, returning `self`.
    pub fn with_stop_word(mut self, stop_word: impl Into<Box<str>>) -> Self {
        self.add_stop_word(stop_word);
        self
    }

    /// Add a stop words.
    pub fn add_stop_words(&mut self, stop_words: impl IntoIterator<Item = impl Into<Box<str>>>) {
        self.stop_words = stop_words.into_iter().map(Into::into).collect();
    }

    /// Add a stop words, returning `self`.
    pub fn with_stop_words(
        mut self,
        stop_words: impl IntoIterator<Item = impl Into<Box<str>>>,
    ) -> Self {
        self.add_stop_words(stop_words);
        self
    }
}

impl From<&ProcessorConfig> for TextProcessor {
    fn from(value: &ProcessorConfig) -> Self {
        TextProcessor::new(value)
    }
}

impl From<ProcessorConfig> for TextProcessor {
    fn from(value: ProcessorConfig) -> Self {
        TextProcessor::new(&value)
    }
}

impl Default for TextProcessor {
    fn default() -> Self {
        let config = ProcessorConfig::default();
        Self::new(&config)
    }
}

impl TextProcessor {
    /// Create a new text processor with configuration
    pub fn new(config: &ProcessorConfig) -> Self {
        TextProcessor {
            min_length: config.min_length.unwrap_or(3),
            max_length: config.max_length.unwrap_or(20),
            enable_emojis: config.enable_emojis.unwrap_or(true),
            stop_words: config.stop_words.iter().cloned().collect(),
        }
    }
}
