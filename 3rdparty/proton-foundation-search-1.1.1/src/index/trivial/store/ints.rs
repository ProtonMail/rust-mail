use crate::entry::EntryValue;

impl TryFrom<&EntryValue> for u64 {
    type Error = ();
    fn try_from(value: &EntryValue) -> Result<Self, Self::Error> {
        match value {
            EntryValue::Integer(v) => Ok(*v),
            _ => Err(()),
        }
    }
}

impl From<u64> for EntryValue {
    fn from(value: u64) -> Self {
        Self::Integer(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::index::prelude::IndexStore;
    use crate::index::trivial::Trivial;

    #[test]
    fn impl_store() {
        fn test(_index: &dyn IndexStore) {}
        test(&Trivial::<u64>::default())
    }
}
