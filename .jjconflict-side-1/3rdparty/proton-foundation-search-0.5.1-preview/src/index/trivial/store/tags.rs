use crate::entry::EntryValue;

impl TryFrom<&EntryValue> for Box<str> {
    type Error = ();
    fn try_from(value: &EntryValue) -> Result<Self, Self::Error> {
        match value {
            EntryValue::Tag(v) => Ok(v.clone()),
            _ => Err(()),
        }
    }
}

impl From<Box<str>> for EntryValue {
    fn from(value: Box<str>) -> Self {
        Self::Tag(value)
    }
}

impl<'a> From<&'a str> for EntryValue {
    fn from(value: &'a str) -> Self {
        EntryValue::Tag(value.into())
    }
}

#[cfg(test)]
mod tests {
    use crate::index::prelude::IndexStore;
    use crate::index::trivial::Trivial;

    #[test]
    fn impl_store() {
        fn test(_index: &dyn IndexStore) {}
        test(&Trivial::<Box<str>>::default())
    }
}
