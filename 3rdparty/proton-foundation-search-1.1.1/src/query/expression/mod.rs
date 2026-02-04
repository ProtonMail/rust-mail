#![doc = include_str!("README.md")]

// PEST PARSER
mod pest_parser;
use core::fmt::Display;
use core::str::FromStr;
use std::convert::Infallible;
use std::iter::once;

pub use pest_parser::QueryParseError;

#[cfg(feature = "wasm-bindgen")]
pub mod wasm;

use serde::{Deserialize, Serialize};

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
        value: TermValue,
    },
}

impl Default for Expression {
    fn default() -> Self {
        Expression::And(vec![])
    }
}

impl Display for Expression {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let fmt_all_op = |f: &mut core::fmt::Formatter<'_>, expressions: &Vec<Expression>, op| {
            once(&'(' as &dyn Display)
                .chain(
                    expressions
                        .iter()
                        .flat_map(|exp| [&' ', &op as &dyn Display, &' ', exp as &dyn Display])
                        .skip(3),
                )
                .chain([&')' as &dyn Display])
                .try_for_each(|token| token.fmt(f))
        };

        match self {
            Expression::And(expressions) => fmt_all_op(f, expressions, Operator::And),
            Expression::Or(expressions) => fmt_all_op(f, expressions, Operator::Or),
            Expression::Not(expression) => write!(f, "!{expression}"),
            Expression::Term {
                field,
                function,
                value,
            } => {
                if let Some(field) = field {
                    for c in field.chars() {
                        if !c.is_alphanumeric() {
                            '\\'.fmt(f)?;
                        }
                        c.fmt(f)?;
                    }
                }
                if function != &Func::Matches {
                    function.fmt(f)?;
                }
                value.fmt(f)
            }
        }
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

/// Query expression term value
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct TermValue {
    inner: TermValueInner,
}

/// Query expression term value
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[repr(C)]
enum TermValueInner {
    /// Text term parts where wildcard is represented by [`TermValuePart::Wildcard`]
    Text(Vec<TermValuePart>),
    /// Integer value term
    Integer(u64),
    /// Boolean value term
    Boolean(bool),
}

/// A part of search term, either text or wildcard
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[repr(C)]
pub enum TermValuePart {
    /// Wildcard represents optional chunk of text
    Wildcard,
    /// Chunk of text to be searched for
    Text(Box<str>),
}
impl<T: Into<Box<str>>> From<T> for TermValuePart {
    fn from(value: T) -> Self {
        TermValuePart::Text(value.into())
    }
}
impl Default for TermValueInner {
    fn default() -> Self {
        Self::Text(vec![])
    }
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl TermValue {
    /// Create a new Text value starting with a wildcard part
    pub fn wild() -> Self {
        Self {
            inner: TermValueInner::Text(vec![TermValuePart::Wildcard]),
        }
    }

    /// Create a new Text value with a single part
    pub fn text(part: &str) -> Self {
        Self {
            inner: TermValueInner::Text(vec![TermValuePart::Text(part.into())]),
        }
    }

    /// Create a new Integer value
    pub fn int(value: u64) -> Self {
        Self {
            inner: TermValueInner::Integer(value),
        }
    }

    /// Create a new Boolean value
    pub fn bool(value: bool) -> Self {
        Self {
            inner: TermValueInner::Boolean(value),
        }
    }

    /// Add a wildcard to the text
    pub fn wildcard(mut self) -> Self {
        match &mut self.inner {
            TermValueInner::Text(items) => {
                if items.last() != Some(&TermValuePart::Wildcard) {
                    items.push(TermValuePart::Wildcard)
                }
            }
            _ => {
                self.inner = TermValueInner::Text(vec![
                    TermValuePart::Text(self.to_string().into_boxed_str()),
                    TermValuePart::Wildcard,
                ])
            }
        }
        self
    }

    /// Add part to a term value
    pub fn then(self, part: &str) -> Self {
        match self.inner {
            TermValueInner::Text(mut items) => match items.as_mut_slice() {
                [.., TermValuePart::Wildcard] => {
                    items.push(TermValuePart::Text(part.into()));
                    TermValue {
                        inner: TermValueInner::Text(items),
                    }
                }
                [TermValuePart::Text(last)] => Self::parse(&[last, part].concat()),
                [.., TermValuePart::Text(last)] => {
                    *last = [last, part].concat().into_boxed_str();
                    TermValue {
                        inner: TermValueInner::Text(items),
                    }
                }
                [] => Self::parse(part),
            },
            _ => Self::parse(&[&self.to_string(), part].concat()),
        }
    }

    /// Converts to integer if the value contains is an integer, also in a string form.
    pub fn to_integer(&self) -> Option<u64> {
        match &self.inner {
            TermValueInner::Text(_) => self.to_string().parse().ok(),
            TermValueInner::Integer(inner) => Some(*inner),
            TermValueInner::Boolean(bool) => Some(*bool as u64),
        }
    }
    /// Converts to boolean if the value contains is a truthy value, also in a string form.
    pub fn to_boolean(&self) -> Option<bool> {
        match &self.inner {
            TermValueInner::Text(_) => self.to_string().parse().ok(),
            TermValueInner::Integer(inner) => Some(*inner != 0),
            TermValueInner::Boolean(bool) => Some(*bool),
        }
    }
    /// Parse a part into a value
    pub fn parse(part: &str) -> Self {
        part.parse::<bool>()
            .map(TermValue::bool)
            .or_else(|_| part.parse::<u64>().map(TermValue::int))
            .unwrap_or_else(|_| TermValue::text(part))
    }
}
impl TermValue {
    /// Get parts of wildcard text, wildcard being represented by [`TermValuePart::Wildcard`]
    pub fn to_wildcard_text(&self) -> Vec<TermValuePart> {
        match &self.inner {
            TermValueInner::Text(s) => s.clone(),
            _ => vec![TermValuePart::Text(self.to_string().into_boxed_str())],
        }
    }
    /// Get text parts if present
    pub fn wildcard_text(&self) -> Option<&[TermValuePart]> {
        match &self.inner {
            TermValueInner::Text(s) => Some(s.as_slice()),
            _ => None,
        }
    }
}

impl Display for TermValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.inner {
            TermValueInner::Text(term_value_parts) => {
                let quote = term_value_parts.iter().any(|part| match part {
                    TermValuePart::Wildcard => false,
                    TermValuePart::Text(text) => text.contains(|c: char| !c.is_alphanumeric()),
                });
                if quote {
                    '"'.fmt(f)?;
                }
                term_value_parts.iter().try_for_each(|part| match part {
                    TermValuePart::Wildcard => '*'.fmt(f),
                    TermValuePart::Text(text) => {
                        if quote {
                            text.chars().try_for_each(|c| match c {
                                '*' | '\\' | '"' => write!(f, "\\{c}"),
                                _ => c.fmt(f),
                            })
                        } else {
                            text.fmt(f)
                        }
                    }
                })?;
                if quote {
                    '"'.fmt(f)?
                }
                Ok(())
            }
            TermValueInner::Integer(v) => v.fmt(f),
            TermValueInner::Boolean(v) => v.fmt(f),
        }
    }
}

impl FromStr for TermValue {
    type Err = Infallible;

    fn from_str(part: &str) -> Result<Self, Infallible> {
        // single string
        Ok(Self::parse(part))
    }
}

impl AsRef<str> for TermValuePart {
    fn as_ref(&self) -> &str {
        match self {
            TermValuePart::Wildcard => "*",
            TermValuePart::Text(text) => text.as_ref(),
        }
    }
}

impl From<u64> for TermValue {
    fn from(value: u64) -> Self {
        Self::int(value)
    }
}
impl From<bool> for TermValue {
    fn from(value: bool) -> Self {
        Self::bool(value)
    }
}
impl From<Box<str>> for TermValue {
    fn from(value: Box<str>) -> Self {
        Self {
            inner: TermValueInner::Text(vec![TermValuePart::Text(value)]),
        }
    }
}
impl From<&str> for TermValue {
    fn from(value: &str) -> Self {
        Self::text(value)
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
impl Display for Operator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Operator::And => "AND".fmt(f),
            Operator::Or => "OR".fmt(f),
        }
    }
}

/// Term impls
impl Expression {
    /// Create a new conditional expression
    #[inline]
    pub fn any_attr(function: Func, value: impl Into<TermValue>) -> Self {
        Self::Term {
            field: None,
            function,
            value: value.into(),
        }
    }
    /// Create a new conditional expression for an attribute
    #[inline]
    pub fn attr(field: impl Into<Box<str>>, function: Func, value: impl Into<TermValue>) -> Self {
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
    /// >
    GreaterThan,
    /// >=
    GreaterThanOrEqual,
    /// <
    LessThan,
    /// <=
    LessThanOrEqual,
}

impl Display for Func {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Func::Matches => "~",
            Func::Equals => "=",
            Func::GreaterThan => ">",
            Func::GreaterThanOrEqual => ">=",
            Func::LessThan => "<",
            Func::LessThanOrEqual => "<=",
        }
        .fmt(f)
    }
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
        value: TermValue::bool(true),
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
