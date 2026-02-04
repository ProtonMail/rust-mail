use crate::blob::BlobId;
use crate::index::prelude::*;
use crate::index::trivial::Trivial;
use crate::index::trivial::search::Finder;
use crate::query::expression::{Func, TermValue, TermValuePart};
use crate::query::option::QueryOptions;
use crate::query::option::results::{CollectPositions, CollectStats};

impl IndexSearch for Trivial<Box<str>> {
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
        match function {
            Func::Matches | Func::Equals => Some(Box::new(Finder::new(
                self,
                revision,
                attribute,
                value.clone(),
                collect_stats,
                collect_positions,
            ))),
            Func::GreaterThan
            | Func::GreaterThanOrEqual
            | Func::LessThan
            | Func::LessThanOrEqual => None,
        }
    }
}

impl Filter<Box<str>> for TermValue {
    fn filter<'a, T>(
        &'a self,
        source: &'a std::collections::BTreeMap<Box<str>, T>,
    ) -> impl Iterator<Item = (&'a Box<str>, &'a T)> {
        let parts = self.to_wildcard_text();
        let first = parts.first().cloned();
        // optimize for prefix search with the first part
        let (taken, range) = match first {
            Some(TermValuePart::Text(part)) => (
                part.len(),
                Box::new(
                    source
                        .range(part.clone()..)
                        .take_while(move |(tag, _)| tag.starts_with(part.as_ref())),
                ) as Box<dyn Iterator<Item = (&Box<str>, _)>>,
            ),
            Some(TermValuePart::Wildcard) | None => (
                0,
                Box::new(source.iter()) as Box<dyn Iterator<Item = (&Box<str>, _)>>,
            ),
        };
        // narrow down the range with subsequent parts
        range.filter(move |(tag, _)| {
            let mut taken = taken;
            for part in parts.iter().skip(1) {
                match part {
                    TermValuePart::Wildcard => {
                        // ignore empty and wildcard parts
                        continue;
                    }
                    TermValuePart::Text(part) => {
                        let Some(found) = tag[taken..].find(part.as_ref()) else {
                            // part was not found, quit
                            return false;
                        };

                        // part was found, update offset and continue
                        taken = found + part.len();
                    }
                }
            }
            if let Some(TermValuePart::Text(last)) = parts.last() {
                // check the tail if not a wildcard
                if !tag.ends_with(last.as_ref()) {
                    return false;
                }
            }
            true
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::index::prelude::{Filter, IndexSearch};
    use crate::index::trivial::Trivial;
    use crate::query::expression::TermValue;

    #[test]
    fn impl_store() {
        fn test(_index: &dyn IndexSearch) {}
        test(&Trivial::<Box<str>>::default())
    }

    #[test]
    fn prefix_filter_simple() {
        let sut = TermValue::text("a").wildcard();

        let source: BTreeMap<Box<str>, _> = [
            ("abc".into(), "abc"),
            ("aaa".into(), "aaa"),
            ("bab".into(), "bab"),
            ("AaA".into(), "AaA"),
        ]
        .into();
        let result = sut.filter(&source).collect::<Vec<_>>();
        insta::assert_debug_snapshot!(result, @r###"
            [
                (
                    "aaa",
                    "aaa",
                ),
                (
                    "abc",
                    "abc",
                ),
            ]
            "###);
    }

    #[test]
    fn prefix_filter() {
        let sut = TermValue::text("/some/path/").wildcard();

        let source: BTreeMap<Box<str>, _> = [
            ("abc".into(), "earlier"),
            ("/some/path/".into(), "exact"),
            ("/some/path/child".into(), "child"),
            ("/some/zzz".into(), "later"),
        ]
        .into();
        let result = sut.filter(&source).collect::<Vec<_>>();
        insta::assert_debug_snapshot!(result, @r###"
            [
                (
                    "/some/path/",
                    "exact",
                ),
                (
                    "/some/path/child",
                    "child",
                ),
            ]
            "###);
    }
}
