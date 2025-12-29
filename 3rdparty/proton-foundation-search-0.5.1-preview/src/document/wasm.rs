//! Explicitly WASM specific implementations and types

use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;

use super::*;

#[wasm_bindgen]
impl Document {
    /// Initiates a document with an identifier.
    #[wasm_bindgen(js_name = "identifier")]
    pub fn id(&self) -> String {
        self.identifier.to_string()
    }

    /// Adds a attribute value to the current document
    ///
    /// A document can have multiple time the same attribute so this function can be called multiple times,
    /// the values will have their own index.
    ///
    /// Wrapper around [add_attribute](proton_foundation_search::writer::document::Document::add_attribute).
    #[wasm_bindgen(js_name = "addAttribute")]
    pub fn add_attribute_value(&mut self, field: &str, value: Value) {
        self.add_attribute(field, value.0)
    }
}

/// One of the few supported value types
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct Value(super::Value);

#[wasm_bindgen]
impl Value {
    /// Create a new Value from a JavaScript value.
    ///
    /// Fails on unsupported types.
    #[wasm_bindgen(constructor)]
    pub fn new(value: JsValue) -> Result<Value, InvalidValue> {
        if let Some(s) = value.as_string() {
            Ok(Value::text(&s))
        } else if let Some(n) = value.as_f64() {
            Ok(Value::int(n as u64))
        } else if let Some(b) = value.as_bool() {
            Ok(Value::bool(b))
        } else if value.is_null() || value.is_undefined() {
            Ok(Value::text(""))
        } else {
            Err(InvalidValue(value))
        }
    }

    /// Create a text value - text means free flowing sequence of words
    pub fn text(value: &str) -> Self {
        Self(super::Value::text(value.to_owned()))
    }
    /// Create a tag value - tag is a specific marker/label
    pub fn tag(value: &str) -> Self {
        Self(super::Value::tag(value.to_owned()))
    }
    /// Create an integer value
    pub fn int(value: u64) -> Self {
        Self(super::Value::Integer(value))
    }
    /// Create a boolean value
    pub fn bool(value: bool) -> Self {
        Self(super::Value::Boolean(value))
    }
}

impl From<Value> for super::Value {
    fn from(value: Value) -> super::Value {
        value.0
    }
}

impl From<super::Value> for Value {
    fn from(value: super::Value) -> Self {
        Self(value)
    }
}

/// Error when an unsupported javascript value is given
#[wasm_bindgen]
#[derive(Debug, thiserror::Error)]
#[error("Unsupported value type")]
pub struct InvalidValue(JsValue);
