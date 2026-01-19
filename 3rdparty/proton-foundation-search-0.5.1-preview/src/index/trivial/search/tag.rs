use crate::index::prelude::*;
use crate::index::trivial::search::Finder;
use crate::index::trivial::{IntoValue, Trivial};
use crate::query::expression::Func;
use crate::query::option::QueryOptions;

impl IndexSearch for Trivial<Box<str>> {
    fn search(
        &self,
        revision: u64,
        attribute: Option<AttributeIndex>,
        function: Func,
        value: &Value,
        _options: &QueryOptions,
    ) -> Option<Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>> {
        let attribute = attribute?;
        match function {
            Func::Matches | Func::Equals => Some(Box::new(Finder::new(
                revision,
                self,
                attribute,
                value.to_string(),
            ))),
            Func::Prefix => Some(Box::new(Finder::new(
                revision,
                self,
                attribute,
                TagPrefixFilter(value.to_string()),
            ))),
            Func::GreaterThan
            | Func::GreaterThanOrEqual
            | Func::LessThan
            | Func::LessThanOrEqual => None,
        }
    }
}

impl IntoValue for Box<str> {
    fn into_value(self) -> Value {
        Value::Tag(self.to_string().into())
    }
}

#[derive(Debug)]
struct TagPrefixFilter(Box<str>);

impl Filter<Box<str>> for TagPrefixFilter {
    fn get<'a, T>(
        &'a self,
        source: &'a std::collections::BTreeMap<Box<str>, T>,
    ) -> impl Iterator<Item = (&'a Box<str>, &'a T)> {
        source
            .range(self.0.clone()..)
            .take_while(|(k, _v)| k.as_ref().starts_with(self.0.as_ref()))
    }
}

#[cfg(test)]
mod tests {
    use crate::index::prelude::{Filter, IndexSearch};
    use crate::index::trivial::Trivial;
    use crate::index::trivial::search::tag::TagPrefixFilter;

    #[test]
    fn impl_store() {
        fn test(_index: &dyn IndexSearch) {}
        test(&Trivial::<Box<str>>::default())
    }

    #[test]
    fn prefix_filter_simple() {
        let sut = TagPrefixFilter("a".into());

        let source = [
            ("abc".into(), "abc"),
            ("aaa".into(), "aaa"),
            ("bab".into(), "bab"),
            ("AaA".into(), "AaA"),
        ]
        .into();
        let result = sut.get(&source).collect::<Vec<_>>();
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
        let sut = TagPrefixFilter("/some/path/".into());

        let source = [
            ("abc".into(), "earlier"),
            ("/some/path/".into(), "exact"),
            ("/some/path/child".into(), "child"),
            ("/some/zzz".into(), "later"),
        ]
        .into();
        let result = sut.get(&source).collect::<Vec<_>>();
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
