//! An index writer's documents.

mod value;
#[cfg(feature = "wasm-bindgen")]
pub mod wasm;

pub use self::value::Value;

/// Container of a document.
///
/// # Example
///
/// ```rust
/// use proton_foundation_search::document::*;
///
/// let _document = Document::new("/foo.txt")
///     .with_attribute("filename", Value::text("Hello World"))
///     .with_attribute("creation", 123456u64);
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
pub struct Document {
    pub(crate) identifier: Box<str>,
    pub(crate) attributes: Vec<(String, Value)>,
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl Document {
    /// Initiates a document with an identifier.
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(constructor)
    )]
    pub fn new(identifier: &str) -> Self {
        Self {
            identifier: identifier.into(),
            attributes: Default::default(),
        }
    }
}
impl Document {
    /// Returns the document identifier.
    pub fn identifier(&self) -> &str {
        self.identifier.as_ref()
    }

    /// Returns the number of attributes defined.
    pub fn attribute_count(&self) -> usize {
        self.attributes.len()
    }

    /// Adds an `value` for the corresponding `field`'s attribute.
    ///
    /// This doesn't overrides previously set attributes in the document
    /// without taking ownership on the object
    #[tracing::instrument(skip_all)]
    pub fn add_attribute<V>(&mut self, field: impl ToString, value: V)
    where
        V: Into<Value>,
    {
        let field = field.to_string();
        let value = value.into();
        self.attributes.push((field, value));
    }

    /// Adds an `value` for the corresponding `field`'s attribute, returning `self`.
    ///
    /// This doesn't overrides previously set attributes in the document
    #[inline]
    pub fn with_attribute<V>(mut self, field: impl ToString, value: V) -> Self
    where
        V: Into<Value>,
    {
        self.add_attribute(field, value);
        self
    }

    /// Returns an iterator over all the available attributes.
    pub fn get_attributes(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.attributes.iter().map(|(k, v)| (k.as_str(), v))
    }
}

#[cfg(test)]
mod tests {
    use test_log::test;

    use super::*;

    #[test]
    fn value_conversion() {
        let value = Value::Integer(42);
        assert_eq!(value.as_integer(), Some(42));
    }

    #[test]
    fn should_build_document() {
        let mut document =
            Document::new("foo.txt").with_attribute("filename", Value::text("foo.txt"));
        document.add_attribute("creation", 123456u64);
        assert_eq!(document.identifier(), "foo.txt");
        assert_eq!(document.attribute_count(), 2);
    }

    #[test]
    fn should_build_document_with_attribute_multiple_times() {
        let document = Document::new("foo.txt")
            .with_attribute("filename", Value::text("foo.txt"))
            .with_attribute("content", Value::text("hello world"))
            .with_attribute("content", Value::text("this is another value"))
            .with_attribute("creation", 42);

        let attrs = document.get_attributes().collect::<Vec<_>>();
        assert_eq!(
            attrs,
            vec![
                ("filename", &Value::text("foo.txt")),
                ("content", &Value::text("hello world")),
                ("content", &Value::text("this is another value")),
                ("creation", &Value::Integer(42))
            ]
        );
    }

    #[test]
    fn should_iterate_over_attributes() {
        let document = Document::new("foo.txt")
            .with_attribute("filename", Value::text("foo.txt"))
            .with_attribute("creation", 42)
            .with_attribute("fancy", Value::text("smile"));

        let attrs = document.get_attributes().collect::<Vec<_>>();
        assert_eq!(
            attrs,
            vec![
                ("filename", &Value::text("foo.txt")),
                ("creation", &Value::Integer(42)),
                ("fancy", &Value::text("smile"))
            ]
        );
    }
}
