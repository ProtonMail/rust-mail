use serde::{Deserialize, Serialize};

/// Representation of an attribute or query  value.
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Value {
    /// A text value.
    Text(Box<str>),
    /// A tag value.
    Tag(Box<str>),
    /// An integer value.
    Integer(u64),
    /// A boolean value.
    Boolean(bool),
}

impl Value {
    /// Create a text value - a free flowing sequence of words
    pub fn tag(value: impl Into<Box<str>>) -> Self {
        Self::Tag(value.into())
    }
    /// Create a tag - a specific marker/label
    pub fn text(value: impl Into<Box<str>>) -> Self {
        Self::Text(value.into())
    }
}

impl From<&u64> for Value {
    fn from(value: &u64) -> Self {
        Self::Integer(*value)
    }
}
impl From<&bool> for Value {
    fn from(value: &bool) -> Self {
        Self::Boolean(*value)
    }
}
impl From<u64> for Value {
    fn from(value: u64) -> Self {
        Self::Integer(value)
    }
}
impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

impl Value {
    /// Converts to integer if the content is already an integer.
    pub fn as_integer(&self) -> Option<u64> {
        match self {
            Self::Integer(inner) => Some(*inner),
            _ => None,
        }
    }
    /// Converts to integer if the value contains is an integer, also in a string form.
    pub fn to_integer(&self) -> Option<u64> {
        match self {
            Self::Text(value) => value.as_ref().parse().ok(),
            Self::Tag(value) => value.as_ref().parse().ok(),
            Self::Integer(inner) => Some(*inner),
            Self::Boolean(bool) => Some(*bool as u64),
        }
    }
    /// Converts to boolean if the value contains is a truthy value, also in a string form.
    pub fn to_boolean(&self) -> Option<bool> {
        match self {
            Self::Text(value) => value.as_ref().parse().ok(),
            Self::Tag(value) => value.as_ref().parse().ok(),
            Self::Integer(inner) => Some(*inner != 0),
            Self::Boolean(bool) => Some(*bool),
        }
    }
    /// Converts the value to its string representation
    pub fn to_string(&self) -> Box<str> {
        match self {
            Value::Text(s) => s.as_ref().into(),
            Value::Tag(s) => s.as_ref().into(),
            Value::Integer(i) => i.to_string().into_boxed_str(),
            Value::Boolean(b) => b.to_string().into_boxed_str(),
        }
    }
}
