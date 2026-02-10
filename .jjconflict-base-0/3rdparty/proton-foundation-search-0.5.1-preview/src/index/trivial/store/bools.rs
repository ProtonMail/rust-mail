use crate::entry::EntryValue;

impl TryFrom<&EntryValue> for bool {
    type Error = ();
    fn try_from(value: &EntryValue) -> Result<Self, Self::Error> {
        match value {
            EntryValue::Boolean(v) => Ok(*v),
            _ => Err(()),
        }
    }
}

impl From<bool> for EntryValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::index::prelude::IndexStore;
    use crate::index::trivial::Trivial;

    #[test]
    fn impl_store() {
        fn test(_index: &dyn IndexStore) {}
        test(&Trivial::<bool>::default())
    }
}
