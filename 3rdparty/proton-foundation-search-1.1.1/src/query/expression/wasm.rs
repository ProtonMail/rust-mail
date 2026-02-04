//! Explicitly WASM specific implementations and types

use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;

use super::{Func, TermValue};

/// A structured expression for query search
#[derive(Default)]
#[wasm_bindgen]
pub struct Expression(super::Expression);

impl From<super::Expression> for Expression {
    fn from(value: super::Expression) -> Self {
        Self(value)
    }
}

impl From<Expression> for super::Expression {
    fn from(value: Expression) -> Self {
        value.0
    }
}

#[wasm_bindgen]
impl Expression {
    /// Create a new empty expression
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self(super::Expression::And(vec![]))
    }

    /// Create a search expression for any attribute
    #[wasm_bindgen(js_name = "anyAttr")]
    pub fn any_attr(function: Func, value: TermValue) -> Self {
        Self(super::Expression::any_attr(function, value))
    }

    /// Create a search expression for a specific attribute
    #[wasm_bindgen]
    pub fn attr(name: &str, function: Func, value: TermValue) -> Self {
        Self(super::Expression::attr(name, function, value))
    }

    /// Combine two expressions with AND
    #[wasm_bindgen]
    pub fn and(self, other: Expression) -> Self {
        Self(super::Expression::and(self.0, other.0))
    }

    /// Combine two expressions with OR
    #[wasm_bindgen]
    pub fn or(self, other: Expression) -> Self {
        Self(super::Expression::or(self.0, other.0))
    }

    /// Negate an expression
    #[wasm_bindgen]
    #[allow(
        clippy::should_implement_trait,
        reason = "std::ops::Not shall not be implemented."
    )]
    pub fn not(self) -> Self {
        Self(super::Expression::not(self.0))
    }
}

#[wasm_bindgen]
impl TermValue {
    /// Create a new Value from a JavaScript value.
    ///
    /// Fails on unsupported types.
    #[wasm_bindgen(constructor)]
    pub fn new(value: JsValue) -> Result<Self, InvalidTermValue> {
        if let Some(s) = value.as_string() {
            Ok(Self::text(&s))
        } else if let Some(n) = value.as_f64() {
            Ok(Self::int(n as u64))
        } else if let Some(b) = value.as_bool() {
            Ok(Self::bool(b))
        } else if value.is_null() || value.is_undefined() {
            Ok(Self::text(""))
        } else {
            Err(InvalidTermValue(value))
        }
    }
}

/// Error when an unsupported javascript value is given
#[wasm_bindgen]
#[derive(Debug, thiserror::Error)]
#[error("Unsupported term value type")]
pub struct InvalidTermValue(JsValue);
