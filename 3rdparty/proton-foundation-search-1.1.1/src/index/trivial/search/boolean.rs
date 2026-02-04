use crate::blob::BlobId;
use crate::index::prelude::*;
use crate::index::trivial::Trivial;
use crate::index::trivial::search::Finder;
use crate::query::expression::{Func, TermValue, TermValuePart};
use crate::query::option::QueryOptions;
use crate::query::option::results::{CollectPositions, CollectStats};

impl IndexSearch for Trivial<bool> {
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
            return Some(Box::new(Finder::new(
                self,
                revision,
                attribute,
                (),
                collect_stats,
                collect_positions,
            )));
        }

        let value = value.to_boolean()?;

        let filter = match function {
            Func::Matches | Func::Equals => value,
            Func::GreaterThan
            | Func::GreaterThanOrEqual
            | Func::LessThan
            | Func::LessThanOrEqual => return None,
        };
        Some(Box::new(Finder::new(
            self,
            revision,
            attribute,
            filter,
            collect_stats,
            collect_positions,
        )))
    }
}

/// Matches any value
impl Filter<bool> for () {
    fn filter<'a, T>(
        &'a self,
        source: &'a std::collections::BTreeMap<bool, T>,
    ) -> impl Iterator<Item = (&'a bool, &'a T)> {
        source.iter()
    }
}

#[cfg(test)]
mod tests {
    use crate::index::prelude::IndexSearch;
    use crate::index::trivial::Trivial;

    #[test]
    fn impl_store() {
        fn test(_index: &dyn IndexSearch) {}
        test(&Trivial::<bool>::default())
    }
}
