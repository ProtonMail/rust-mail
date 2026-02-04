use std::collections::BTreeMap;
use std::fmt::Debug;
use std::ops::{Bound, RangeBounds};

use serde::Deserialize;

use crate::blob::BlobId;
use crate::index::prelude::*;
use crate::index::trivial::search::Finder;
use crate::index::trivial::{IndexableValue, Trivial};
use crate::query::expression::{Func, TermValue, TermValuePart};
use crate::query::option::QueryOptions;
use crate::query::option::results::{CollectPositions, CollectStats};

fn range<
    R: RangeBounds<V> + Clone + Debug + Send + 'static,
    V: IndexableValue + for<'de> Deserialize<'de>,
>(
    index: &Trivial<V>,
    revision: BlobId,
    attr: AttributeIndex,
    range: R,
    collect_stats: bool,
    collect_positions: bool,
) -> Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>> {
    Box::new(Finder::new(
        index,
        revision,
        attr,
        Range(range),
        collect_stats,
        collect_positions,
    )) as Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>
}
fn equal<V: IndexableValue + for<'de> Deserialize<'de>>(
    index: &Trivial<V>,
    revision: BlobId,
    attr: AttributeIndex,
    value: V,
    collect_stats: bool,
    collect_positions: bool,
) -> Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>> {
    Box::new(Finder::new(
        index,
        revision,
        attr,
        value,
        collect_stats,
        collect_positions,
    )) as Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>
}

impl IndexSearch for Trivial<u64> {
    fn search(
        &self,
        revision: BlobId,
        attribute: Option<AttributeIndex>,
        function: Func,
        value: &TermValue,
        options: &QueryOptions,
    ) -> Option<Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>> {
        let attribute = attribute?;
        let collect_stats = CollectStats::get(options);
        let collect_positions = CollectPositions::get(options);

        if let Some([TermValuePart::Wildcard]) = value.wildcard_text() {
            // existential check shortcut: entry matches if it has any value
            return Some(range(
                self,
                revision,
                attribute,
                0..,
                collect_stats,
                collect_positions,
            ));
        }

        let value = value.to_integer()?;

        Some(match function {
            Func::Matches | Func::Equals => equal(
                self,
                revision,
                attribute,
                value,
                collect_stats,
                collect_positions,
            ),
            Func::GreaterThan => range(
                self,
                revision,
                attribute,
                GT(value),
                collect_stats,
                collect_positions,
            ),
            Func::GreaterThanOrEqual => range(
                self,
                revision,
                attribute,
                value..,
                collect_stats,
                collect_positions,
            ),
            Func::LessThan => range(
                self,
                revision,
                attribute,
                ..value,
                collect_stats,
                collect_positions,
            ),
            Func::LessThanOrEqual => range(
                self,
                revision,
                attribute,
                ..=value,
                collect_stats,
                collect_positions,
            ),
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
    fn filter<'a, T>(&'a self, source: &'a BTreeMap<V, T>) -> impl Iterator<Item = (&'a V, &'a T)> {
        let Self(range) = self;
        source.range(range.clone())
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
