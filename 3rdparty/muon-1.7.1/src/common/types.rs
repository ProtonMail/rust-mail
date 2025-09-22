//! ## Types
//!
//! This module defines core types used throughout the `muon` crate.

use derive_more::{Debug, Display, From, Into};
use futures::prelude::*;
use itertools::Itertools;
use muon_proc::autoimpl;
use std::any::{type_name, Any, TypeId};
use std::collections::HashMap;
use std::error::Error as StdError;
use std::fmt::{Formatter, Result};
use std::hash::{Hash, Hasher};
use std::ops::{Add, Mul};

/// A boxed error type; the underlying error type is erased.
pub type BoxErr = Box<dyn StdError + Send + Sync>;

// The wasm target needs a non-`Send` future type.
if_wasm! {{
    /// A boxed future type; the underlying future type is erased.
    pub type BoxFut<'a, T> = future::LocalBoxFuture<'a, T>;
} else {
    /// A boxed future type; the underlying future type is erased.
    pub type BoxFut<'a, T> = future::BoxFuture<'a, T>;
}}

/// A type map: enables storing and retrieving types by type.
///
/// This is useful for storing types that are not known at compile time,
/// such as metadata for a request/response as it passes through layers.
#[derive(Debug, Display, Default, From, Into, Clone)]
#[debug("{:?}", map.keys())]
#[display("[{}]", map.keys().join(", "))]
pub struct TypeMap {
    map: HashMap<TypeName, BoxObj>,
}

impl TypeMap {
    /// Insert a type into this type map.
    /// If a type of this type already existed, it will be returned.
    pub fn insert<T: Clone + Send + Sync + 'static>(&mut self, val: T) -> Option<T> {
        let k = TypeName::of::<T>();
        let v = Box::new(val);

        if let Some(t) = self.map.insert(k, v) {
            t.into_any().downcast().ok().map(|t| *t)
        } else {
            None
        }
    }

    /// Get a reference to a type previously inserted on this type map.
    /// If the type was not found, `None` is returned.
    #[must_use]
    pub fn get<T: 'static>(&self) -> Option<&T> {
        let k = TypeName::of::<T>();
        let t = self.map.get(&k);

        if let Some(t) = t {
            t.as_ref().as_any().downcast_ref()
        } else {
            None
        }
    }

    /// Get a mutable reference to a type previously inserted on this type map.
    /// If the type was not found, `None` is returned.
    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        let k = TypeName::of::<T>();
        let t = self.map.get_mut(&k);

        if let Some(t) = t {
            t.as_mut().as_any_mut().downcast_mut()
        } else {
            None
        }
    }

    /// Remove a type from this type map.
    /// If the type was found, it is returned.
    pub fn remove<T: 'static>(&mut self) -> Option<T> {
        let k = TypeName::of::<T>();
        let t = self.map.remove(&k);

        if let Some(t) = t {
            t.into_any().downcast().ok().map(|t| *t)
        } else {
            None
        }
    }
}

/// `TypeName` is like `TypeId` but records the name of the type as well.
#[derive(Debug, Display, Clone, Copy)]
#[debug("{name}")]
#[display("{name}")]
struct TypeName {
    id: TypeId,
    name: &'static str,
}

impl TypeName {
    fn of<T: 'static>() -> Self {
        Self {
            id: TypeId::of::<T>(),
            name: type_name::<T>(),
        }
    }
}

impl PartialEq for TypeName {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for TypeName {
    // ...
}

impl Hash for TypeName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// A boxed object.
type BoxObj = Box<dyn Obj + Send + Sync>;

/// A boxed any.
type BoxAny = Box<dyn Any>;

/// `Obj` is like `Any` but better.
trait Obj: Any {
    /// `self` can be anything; return a clone of it as a boxed `Obj`.
    fn clone(&self) -> BoxObj;

    /// Return a reference to the `Any` trait object.
    fn as_any(&self) -> &dyn Any;

    /// Return a mutable reference to the `Any` trait object.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Convert `self` (a boxei concrete type) into a boxed `Any` trait object.
    fn into_any(self: Box<Self>) -> BoxAny;
}

impl<T: Clone + Any + Send + Sync> Obj for T {
    fn clone(&self) -> BoxObj {
        Box::new(Clone::clone(self))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> BoxAny {
        self
    }
}

impl Clone for BoxObj {
    fn clone(&self) -> Self {
        // Get the trait object.
        let this: &dyn Obj = &**self;

        // Clone the trait object into a boxed trait object.
        let this: BoxObj = Obj::clone(this);

        // Return the cloned trait object.
        this
    }
}

impl Debug for BoxObj {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "{:?}", self.as_any())
    }
}

/// A trait for a type that can enumerate all its possible values.
pub trait TypeIter: Clone {
    /// Iterate through all possible values.
    fn iter() -> impl Iterator<Item = Self> + Clone;
}

/// Re-export the `TypeIter` derive macro.
pub use muon_proc::TypeIter;

/// Simple impls for fundamental types.
impl TypeIter for bool {
    fn iter() -> impl Iterator<Item = Self> + Clone {
        [false, true].into_iter()
    }
}

/// An iterator that generates values in a range.
#[derive(Debug, Clone)]
pub struct RangeIter<T> {
    n: T,
    end: T,
    add: T,
    mul: T,
}

impl<T> RangeIter<T> {
    /// Create a new range iterator.
    pub fn new(n: T, end: T, add: T, mul: T) -> Self {
        Self { n, end, add, mul }
    }
}

impl<T: Copy> Iterator for RangeIter<T>
where
    T: Mul<Output = T>,
    T: Add<Output = T>,
    T: PartialOrd,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.n > self.end {
            None
        } else {
            Some(self.n.replace(|&n| n * self.mul + self.add))
        }
    }
}

#[autoimpl]
trait Replace: Clone {
    fn replace(&mut self, f: impl FnOnce(&Self) -> Self) -> Self {
        let prev = self.clone();

        *self = f(&prev);

        prev
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_map() {
        let mut m = TypeMap::default();

        m.insert(5i32);
        m.insert("hello");

        println!("Debug: {m:#?}");
        println!("Display: {m}");

        let t = m.get::<i32>();
        assert_eq!(t, Some(&5));

        let t = m.get::<&str>();
        assert_eq!(t, Some(&"hello"));

        let t = m.get_mut::<i32>();
        assert_eq!(t, Some(&mut 5));

        let t = m.get_mut::<&str>();
        assert_eq!(t, Some(&mut "hello"));

        let t = m.remove::<i32>();
        assert_eq!(t, Some(5));
        assert_eq!(m.get::<i32>(), None);

        let t = m.remove::<&str>();
        assert_eq!(t, Some("hello"));
        assert_eq!(m.get::<&str>(), None);
    }
}
