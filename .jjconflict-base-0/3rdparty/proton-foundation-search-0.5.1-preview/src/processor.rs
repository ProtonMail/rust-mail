//! Conversions of input values to indexed values and matching conversion of query input

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::document::{Document, Value};
use crate::entry::{Entry, EntryValue, EntryValues};
use crate::index::text::processor::{TextProcessor, TextProcessorConfig};

/// Trait for value processing
pub trait Proc: std::fmt::Debug + Send + Sync {
    /// Convert Document Values to Entry IndexedValue
    fn process_document(&self, document: Document) -> Result<(usize, Entry), ProcessorError>;

    /// Tokenize query text so it can be matched against IndexedValue text
    fn process_query(&self, query: &str) -> Vec<Box<str>>;
}

impl Proc for Processor {
    fn process_document(&self, document: Document) -> Result<(usize, Entry), ProcessorError> {
        Processor::process_document(self, document)
    }

    fn process_query(&self, query: &str) -> Vec<Box<str>> {
        self.text
            .process(query)
            .into_iter()
            .map(|(_, term)| term)
            .collect()
    }
}

/// Configure the Processor
#[derive(Debug, Default)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct ProcessorConfig {
    /// Text processing configuration
    text: TextProcessorConfig,
}

impl ProcessorConfig {
    /// Create a new constructor config with given values
    pub fn new(
        min_length: Option<usize>,
        max_length: Option<usize>,
        enable_emojis: Option<bool>,
    ) -> Self {
        Self {
            text: TextProcessorConfig {
                min_length,
                max_length,
                enable_emojis,
            },
        }
    }
}

impl From<ProcessorConfig> for Processor {
    fn from(value: ProcessorConfig) -> Self {
        Processor::new(value)
    }
}

/// Errors arising while converting documents or changes into indexable values
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum QueryError {}

/// Errors arising while converting documents or changes into indexable values
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProcessorError {
    /// Invalid field requested
    #[error("The field {0} is not defined in the schema")]
    UndefinedField(Box<str>),
}

/// TODO: this little hack should be removed once we move to sans-io
impl From<ProcessorError> for std::io::Error {
    fn from(value: ProcessorError) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, value)
    }
}

/// The built-in [`Proc`] implementation. You may want to implement your own for your use case.
#[derive(Debug)]
pub struct Processor {
    pub(crate) text: TextProcessor,
}

impl Default for Processor {
    fn default() -> Self {
        Processor::new(ProcessorConfig::default())
    }
}

impl Processor {
    /// Create a new processor with config
    pub fn new(config: ProcessorConfig) -> Self {
        Self {
            text: config.text.build(),
        }
    }

    /// Processes a document value into a list of tokens.
    #[tracing::instrument(skip_all)]
    fn process_value(&self, values: Vec<Value>) -> (usize, EntryValues) {
        let mut processed = vec![];
        let mut size = 0;

        for value in values {
            match value {
                Value::Text(v) => {
                    size += v.len();
                    processed.push(EntryValue::Text(self.text.process(v.as_ref())));
                }
                Value::Tag(v) => {
                    size += v.len();
                    processed.push(EntryValue::Tag(v));
                }
                Value::Integer(v) => {
                    size += std::mem::size_of::<u64>();
                    processed.push(EntryValue::Integer(v));
                }
                Value::Boolean(v) => {
                    size += std::mem::size_of::<u64>();
                    processed.push(EntryValue::Boolean(v));
                }
            }
        }

        (size, processed)
    }

    /// Processes a `document` and its content to be ingestible in the index.
    ///
    /// returns the size of the doc and entry
    #[tracing::instrument(skip_all)]
    pub(crate) fn process_document(
        &self,
        document: Document,
    ) -> Result<(usize, Entry), ProcessorError> {
        let Document {
            identifier,
            attributes,
        } = document;

        let mut total_size = 0;

        let attributes = attributes
            .into_iter()
            .map(|(field, value)| Ok((field, value)))
            .try_fold(BTreeMap::<_, Vec<_>>::new(), |mut map, item| {
                let (name, value) = item?;
                map.entry(name).or_default().push(value);
                Ok(map)
            })?
            .into_iter()
            .map(|(name, values)| {
                let (size, value) = self.process_value(values);
                total_size += size;
                (name.into_boxed_str(), Arc::new(value))
            })
            .collect();

        let entry = Entry::new(identifier.as_ref(), attributes);

        Ok((total_size, entry))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use test_log::test;

    use super::*;
    use crate::document::Document;

    #[test]
    fn getting_partition_value() {
        let title = "title";
        let creation = "creation";

        let processor = Processor {
            text: TextProcessorConfig::default().build(),
        };

        let document = Document::new("foo.txt")
            .with_attribute(title, Value::text("Hello World"))
            .with_attribute(creation, 1234);
        let _processed = processor.process_document(document).expect("entry");

        let document = Document::new("foo.txt").with_attribute(title, Value::text("Hello World"));
        let _processed = processor
            .process_document(document)
            .expect("entry attribute can be missing");

        let document = Document::new("foo.txt")
            .with_attribute(title, Value::text("Hello World"))
            .with_attribute(creation, Value::text("bar"));
        let _processed = processor
            .process_document(document)
            .expect("entry attribute value type does not have to match schema");
    }

    #[test]
    fn entry_partition_getter() {
        assert_eq!(Entry::new("foo", Default::default()).identifier(), "foo");
    }

    #[test]
    fn can_add_more_than_65536_values_in_same_attribute() {
        let testfield = "testfield";
        let creation = "creation";

        let processor = Processor {
            text: TextProcessorConfig::default().build(),
        };

        let mut document = Document::new("foo.txt");
        document.add_attribute(creation, u64::MAX);

        for i in 0..=u16::MAX {
            document.add_attribute(testfield, i as u64);
        }
        processor
            .process_document(document.clone())
            .expect("u16 attr values OK");

        document.add_attribute(testfield, u64::MAX);

        assert!(processor.process_document(document).is_ok());
    }
}
