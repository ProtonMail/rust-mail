//! Search query expressions.
//!
//! This module provides a flexible query DSL (Domain Specific Language) for building and executing
//! search queries across different attribute types. It supports complex boolean operations,
//! field-specific filters, and full-text search.
//!
//! # Core Types
//!
//! The query system is built around these key types:
//!
//! - `Expression`: Composable boolean expressions (AND/OR) of conditions
//!
//! # Example Usage
//!
//! ```rust
//! use proton_foundation_search::document::Value;
//! use proton_foundation_search::engine::{Engine, QueryEvent};
//! use proton_foundation_search::query::expression::{Expression, Func};
//!
//! # fn example(engine: Engine) -> Result<(), Box<dyn std::error::Error>> {
//! # let today = 42;
//!
//! // Simple text search
//! for event in engine
//!     .query()
//!     .with_expression("hello world".parse()?)
//!     .search()
//! {
//!     // Matches are returned early. Scores at the end.
//!     match event {
//!         QueryEvent::Load(_) => todo!(),
//!         QueryEvent::Found(_) => todo!(),
//!         QueryEvent::Stats(_) => todo!(),
//!     }
//! }
//!
//! // Complex boolean query
//! let query_iterator = engine
//!     .query()
//!     .with_expression(Expression::And(vec![
//!         Expression::attr("title", Func::Matches, Value::text("urgent")),
//!         Expression::attr("body", Func::Matches, Value::text("report")),
//!     ]))
//!     .with_expression(Expression::Or(vec![
//!         Expression::attr("date", Func::GreaterThan, today),
//!         Expression::attr("priority", Func::Equals, 1),
//!     ]))
//!     .search();
//! # Ok(())
//! # }
//! ```
//!
//! # Query Building
//!
//! Queries are built using a combination of:
//!
//! - Boolean expressions (AND/OR)
//! - Field conditions with type-specific filters
//! - Full-text search terms
//! - Field boost factors
//!
//! The builder enforces type safety by validating that conditions match field types.
//!
//! # Supported Query Types
//!
//! ## Text Search
//! - Word matching with stemming
//! - Phrase matching
//! - Field-specific or all-fields search
//! - Boosted fields
//!
//! ## Integer Fields
//! - Exact match
//! - Range queries (>, >=, <, <=)
//! - IN/NOT IN lists
//!
//! ## Boolean Fields
//! - True/false matching
//! - IS NULL checks
//!
//! ## Tag Fields
//! - Tag presence
//! - Multiple tag matching
//! - Tag prefix matching
//!
//! # Query Execution
//!
//! A query search is a state machine that must be iterated.
//! In case of cache miss, the machine will request loading blobs from the app.
//!
//! # Performance Considerations
//!
//! Query performance is optimized by:
//!
//! - TODO: optimized execution planning
//! - caching
//!
//! # Error Handling
//!
//! The query builder validates:
//!
//! - Field existence
//! - Type compatibility
//! - Expression validity
//! - Parse errors
//!
//! Runtime errors during search are propagated via `Result`.
//!
//! # Thread Safety
//!
//! Queries are safe for concurrent execution:
//!
//! - Multiple queries can run simultaneously
//! - Query state is immutable after building
//!
//! # Cancellation
//!
//! Queries can be canceled by simply not polling the iterator anymore.

// PEST PARSER
mod pest_parser;
pub use pest_parser::QueryParseError;

#[cfg(feature = "wasm-bindgen")]
pub mod wasm;

use serde::{Deserialize, Serialize};

use crate::document::Value;

/// Represents a query expression tree
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[repr(C)]
pub enum Expression {
    /// Conjunction of two expressions
    And(Vec<Expression>),
    /// Disjunction of two expressions
    Or(Vec<Expression>),
    /// The opposite of the expression
    Not(Box<Expression>),
    /// The actual condition expression
    Term {
        /// The attribute concerned or all
        field: Option<Box<str>>,
        /// The function to apply to attribute values
        function: Func,
        /// The search parameter
        value: Value,
    },
}

impl Default for Expression {
    fn default() -> Self {
        Expression::And(vec![])
    }
}

impl Expression {
    /// Create a negation of an expression
    pub fn not(negated: impl Into<Expression>) -> Self {
        Expression::Not(Box::new(negated.into()))
    }
    /// Create a conjunction of two expressions
    pub fn and(left: impl Into<Expression>, right: impl Into<Expression>) -> Self {
        use Expression::And as Op;
        let left = left.into();
        let right = right.into();

        match (left, right) {
            (Op(mut left), Op(mut right)) => {
                left.append(&mut right);
                Op(left)
            }
            (Op(mut left), right) => {
                left.push(right);
                Op(left)
            }
            (left, Op(mut right)) => {
                right.insert(0, left);
                Op(right)
            }
            (left, right) => Op(vec![left, right]),
        }
    }
    /// Create a disjunction of two expressions
    pub fn or(left: impl Into<Expression>, right: impl Into<Expression>) -> Self {
        use Expression::Or as Op;
        let left = left.into();
        let right = right.into();

        match (left, right) {
            (Op(mut left), Op(mut right)) => {
                left.append(&mut right);
                Op(left)
            }
            (Op(mut left), right) => {
                left.push(right);
                Op(left)
            }
            (left, Op(mut right)) => {
                right.insert(0, left);
                Op(right)
            }
            (left, right) => Op(vec![left, right]),
        }
    }

    pub(crate) fn push(&mut self, expression: Expression) {
        *self = match std::mem::replace(self, Expression::And(vec![])) {
            Expression::And(mut expressions) => {
                expressions.push(expression);
                Expression::And(expressions)
            }
            Expression::Or(mut expressions) => {
                expressions.push(expression);
                Expression::Or(expressions)
            }
            unit @ Expression::Not(_) | unit @ Expression::Term { .. } => {
                Expression::And(vec![unit, expression])
            }
        }
    }
}

impl From<(Expression, Expression)> for Expression {
    fn from((left, right): (Expression, Expression)) -> Self {
        Self::and(left, right)
    }
}

impl From<(Expression, Operator, Expression)> for Expression {
    fn from((left, operator, right): (Expression, Operator, Expression)) -> Self {
        match operator {
            Operator::And => Self::and(left, right),
            Operator::Or => Self::or(left, right),
        }
    }
}

/// An operator between two expressions
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[repr(C)]
pub enum Operator {
    /// Conjunction
    And,
    /// Disjunction
    Or,
}

/// Term impls
impl Expression {
    /// Create a new conditional expression
    #[inline]
    pub fn any_attr(function: Func, value: impl Into<Value>) -> Self {
        Self::Term {
            field: None,
            function,
            value: value.into(),
        }
    }
    /// Create a new conditional expression for an attribute
    #[inline]
    pub fn attr(field: impl Into<Box<str>>, function: Func, value: impl Into<Value>) -> Self {
        Self::Term {
            field: Some(field.into()),
            function,
            value: value.into(),
        }
    }
    /// Check if this expression has no conditions
    pub fn is_empty(&self) -> bool {
        match self {
            Expression::And(expressions) | Expression::Or(expressions) => {
                expressions.iter().all(|exp| exp.is_empty())
            }
            Expression::Not(expression) => expression.is_empty(),
            Expression::Term { .. } => false,
        }
    }
}

/// All the possible query term expression functions
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[repr(C)]
pub enum Func {
    /// Like equal, but in text search, this would be a fuzzy match
    #[default]
    Matches,
    /// Exact match (==)
    Equals,
    /// Starts with search
    Prefix,
    /// >
    GreaterThan,
    /// >=
    GreaterThanOrEqual,
    /// <
    LessThan,
    /// <=
    LessThanOrEqual,
}

#[test]
fn test_is_empty() {
    assert!(Expression::And(vec![]).is_empty());
    assert!(Expression::Or(vec![]).is_empty());
    assert!(Expression::Not(Box::new(Expression::Or(vec![]))).is_empty());
    assert!(
        Expression::And(vec![
            Expression::Or(vec![]),
            Expression::Not(Box::new(Expression::And(vec![])))
        ])
        .is_empty()
    );
}

#[test]
fn test_is_not_empty() {
    let term = Expression::Term {
        field: None,
        function: Func::Equals,
        value: Value::Boolean(true),
    };
    assert!(!term.is_empty());
    assert!(!Expression::And(vec![term.clone()]).is_empty());
    assert!(!Expression::Or(vec![term.clone()]).is_empty());
    assert!(!Expression::Not(Box::new(Expression::Or(vec![term.clone()]))).is_empty());
    assert!(
        !Expression::And(vec![
            Expression::Or(vec![]),
            Expression::Not(Box::new(Expression::And(vec![term.clone()])))
        ])
        .is_empty()
    );
}
