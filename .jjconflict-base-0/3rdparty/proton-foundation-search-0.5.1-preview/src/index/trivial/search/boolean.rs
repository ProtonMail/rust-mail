use crate::index::prelude::*;
use crate::index::trivial::search::Finder;
use crate::index::trivial::{IntoValue, Trivial};
use crate::query::expression::Func;
use crate::query::option::QueryOptions;

impl IndexSearch for Trivial<bool> {
    fn search(
        &self,
        revision: u64,
        attribute: Option<AttributeIndex>,
        function: Func,
        value: &Value,
        _options: &QueryOptions,
    ) -> Option<Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>> {
        let attribute = attribute?;
        let value = value.to_boolean()?;

        let filter = match function {
            Func::Matches | Func::Equals => value,
            Func::GreaterThan
            | Func::GreaterThanOrEqual
            | Func::LessThan
            | Func::LessThanOrEqual
            | Func::Prefix => return None,
        };
        Some(Box::new(Finder::new(revision, self, attribute, filter)))
    }
}

impl IntoValue for bool {
    fn into_value(self) -> Value {
        Value::Boolean(self)
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
