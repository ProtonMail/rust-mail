//! WASM specific code for Entry and EntryValue
use js_sys::Array;
use wasm_bindgen::convert::TryFromJsValue;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsError, JsValue};

use crate::engine::wasm::TextToken;
use crate::entry::Entry;

#[wasm_bindgen]
impl Entry {
    /// Create a new entry with the given identifier
    pub fn new_wasm(id: &str) -> Self {
        Self {
            identifier: id.into(),
            attributes: [].into(),
        }
    }

    /// Get the entry identifier
    #[wasm_bindgen(js_name = "identifier")]
    pub fn identifier_wasm(&self) -> String {
        self.identifier.as_ref().to_owned()
    }

    /// Get an attribute value
    #[wasm_bindgen(js_name = "attribute")]
    pub fn attribute_wasm(&self, attribute: &str) -> Vec<EntryValue> {
        self.attributes
            .get(attribute)
            .map(|a| a.iter().map(|v| v.clone().into()).collect())
            .unwrap_or_default()
    }

    /// Set an attribute value
    #[wasm_bindgen(js_name = "setAttribute")]
    pub fn set_attribute_wasm(&mut self, attribute: &str, values: Vec<EntryValue>) {
        self.attributes.insert(
            attribute.into(),
            values
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>()
                .into(),
        );
    }

    /// Get a list of attribute names
    #[wasm_bindgen(js_name = "attributes")]
    pub fn attributes_wasm(&self) -> Vec<String> {
        self.attributes
            .keys()
            .map(|attr| attr.as_ref().to_owned())
            .collect()
    }
}

/// Entry value represents values recognized by the search engine
#[derive(Debug, Clone)]
#[wasm_bindgen]
pub struct EntryValue(Option<super::EntryValue>);

#[wasm_bindgen]
impl EntryValue {
    /// Create a new EntryValue
    #[wasm_bindgen(constructor)]
    pub fn new_wasm(value: JsValue) -> Result<Self, JsError> {
        if let Some(b) = value.as_bool() {
            Ok(Self(Some(b.into())))
        } else if let Some(i) = value.as_f64() {
            Ok(Self(Some((i as u64).into())))
        } else if let Some(t) = value.as_string() {
            Ok(Self(Some(t.as_str().into())))
        } else if value.is_array() {
            let a = Array::from(&value);
            let mut tokens = vec![];
            for v in a {
                let v = TextToken::try_from_js_value(v)
                    .map_err(|e| JsError::new(&format!("unsupported array value {:?}", e)))?;

                tokens.push((v.position(), v.text().into_boxed_str()));
            }
            Ok(Self(Some(tokens.into())))
        } else {
            Err(JsError::new(&format!("unsupported value {:?}", value)))
        }
    }

    /// Get the EntryValue value
    pub fn value(&self) -> Option<JsValue> {
        Some(match self.0.as_ref()? {
            crate::entry::EntryValue::Text(items) => JsValue::from(
                items
                    .iter()
                    .map(|(p, t)| JsValue::from(TextToken::new(*p, t.as_ref().to_owned())))
                    .collect::<Array>(),
            ),
            crate::entry::EntryValue::Tag(v) => JsValue::from_str(v.as_ref()),
            crate::entry::EntryValue::Integer(v) => JsValue::from_f64(*v as f64),
            crate::entry::EntryValue::Boolean(v) => JsValue::from_bool(*v),
        })
    }
}

impl From<EntryValue> for Option<super::EntryValue> {
    fn from(value: EntryValue) -> Self {
        value.0
    }
}
impl From<Option<super::EntryValue>> for EntryValue {
    fn from(value: Option<super::EntryValue>) -> Self {
        EntryValue(value)
    }
}

#[test]
fn wasm_can_get_and_set_entry_values() {
    let mut sut = Entry::new_wasm("x");
    sut.set_attribute_wasm(
        "a",
        vec![EntryValue(Some(1.into())), EntryValue(Some(true.into()))],
    );
    sut.set_attribute_wasm(
        "b",
        vec![
            EntryValue(Some("tag".into())),
            EntryValue(super::EntryValue::text(["hello", "world"])),
        ],
    );
    let id = sut.identifier_wasm();
    let attrs = sut.attributes_wasm();
    let a = sut.attribute_wasm("a");
    let b = sut.attribute_wasm("b");
    insta::assert_debug_snapshot!((id, attrs, a, b, sut));
}
