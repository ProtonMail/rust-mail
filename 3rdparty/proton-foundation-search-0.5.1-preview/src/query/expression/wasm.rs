//! Explicitly WASM specific implementations and types

use wasm_bindgen::prelude::wasm_bindgen;

use super::Func;
use crate::document::wasm::Value;

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
    pub fn any_attr(function: Func, value: Value) -> Self {
        Self(super::Expression::any_attr(function, value))
    }

    /// Create a search expression for a specific attribute
    #[wasm_bindgen]
    pub fn attr(name: &str, function: Func, value: Value) -> Self {
        Self(super::Expression::attr(name, function, value))
    }

    /// Combine two expressions with AND
    #[wasm_bindgen(js_name = "and")]
    pub fn and(self, other: Expression) -> Self {
        Self(super::Expression::and(self.0, other.0))
    }

    /// Combine two expressions with OR
    #[wasm_bindgen(js_name = "or")]
    pub fn or(self, other: Expression) -> Self {
        Self(super::Expression::or(self.0, other.0))
    }

    /// Negate an expression
    #[wasm_bindgen(js_name = "not")]
    pub fn not(self) -> Self {
        Self(super::Expression::not(self.0))
    }
}
