use std::collections::BTreeMap;
use std::fmt::Debug;
use std::ops::{Bound, RangeBounds};

use serde::Deserialize;

use crate::index::prelude::*;
use crate::index::trivial::search::Finder;
use crate::index::trivial::{IndexableValue, IntoValue, Trivial};
use crate::query::expression::Func;
use crate::query::option::QueryOptions;

fn range<
    R: RangeBounds<V> + Clone + Debug + Send + 'static,
    V: IndexableValue + for<'de> Deserialize<'de>,
>(
    revision: u64,
    index: &Trivial<V>,
    attr: AttributeIndex,
    range: R,
) -> Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>> {
    Box::new(Finder::new(revision, index, attr, Range(range)))
        as Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>
}
fn equal<V: IndexableValue + for<'de> Deserialize<'de>>(
    revision: u64,
    index: &Trivial<V>,
    attr: AttributeIndex,
    value: V,
) -> Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>> {
    Box::new(Finder::new(revision, index, attr, value))
        as Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>
}

impl IndexSearch for Trivial<u64> {
    fn search(
        &self,
        revision: u64,
        attribute: Option<AttributeIndex>,
        function: Func,
        value: &Value,
        _options: &QueryOptions,
    ) -> Option<Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>> {
        let attribute = attribute?;
        let value = value.to_integer()?;

        Some(match function {
            Func::Matches | Func::Equals => equal(revision, self, attribute, value),
            Func::GreaterThan => range(revision, self, attribute, GT(value)),
            Func::GreaterThanOrEqual => range(revision, self, attribute, value..),
            Func::LessThan => range(revision, self, attribute, ..value),
            Func::LessThanOrEqual => range(revision, self, attribute, ..=value),
            Func::Prefix => return None,
        })
    }
}

#[derive(Debug)]
struct Range<R>(R);
#[derive(Debug, Clone)]
struct GT<T>(T);
impl<T> RangeBounds<T> for GT<T> {
    fn start_bound(&self) -> Bound<&T> {
        Bound::Excluded(&self.0)
    }

    fn end_bound(&self) -> std::ops::Bound<&T> {
        Bound::Unbounded
    }
}
impl<V: IndexableValue, R: RangeBounds<V> + Clone + Debug> Filter<V> for Range<R> {
    fn get<'a, T>(&'a self, source: &'a BTreeMap<V, T>) -> impl Iterator<Item = (&'a V, &'a T)> {
        let Self(range) = self;
        source.range(range.clone())
    }
}

impl IntoValue for u64 {
    fn into_value(self) -> Value {
        Value::Integer(self)
    }
}

#[cfg(test)]
mod tests {
    use crate::index::prelude::IndexSearch;
    use crate::index::trivial::Trivial;

    #[test]
    fn impl_store() {
        fn test(_index: &dyn IndexSearch) {}
        test(&Trivial::<u64>::default())
    }
}
