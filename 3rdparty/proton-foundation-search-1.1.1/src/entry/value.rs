use super::*;

/// A list of index values
pub type EntryValues = Vec<Option<EntryValue>>;

/// Entry value represents values recognized by the search engine.
///
/// It is passed to each index and each index takes what it need for the given attribute.
///
/// Schema is defined by the app by inserting entry-attribute-values and technically,
/// an attribute can hold values of different types.
///
/// In order to make a round trip between export/import of entries, the atomic entry-attribute value
/// is a list of different [`EntryValue`], preserving the value index of each value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[repr(C)]
#[serde(untagged)]
pub enum EntryValue {
    /// Tokenized text blob values with their original positions
    Text(Vec<(usize, Box<str>)>),
    /// A tag value.
    Tag(Box<str>),
    /// An integer value.
    Integer(u64),
    /// A boolean value.
    Boolean(bool),
}

#[cfg(any(test, feature = "bench-mode"))]
impl EntryValue {
    /// In test, create Some EntryValue from types that convert into EntryValue.
    pub fn new<V>(value: V) -> Option<Self>
    where
        EntryValue: From<V>,
    {
        Some(value.into())
    }
    /// In test create Some text EntryValue from tokens, if any.
    pub fn text(tokens: impl IntoIterator<Item = impl Into<Box<str>>>) -> Option<Self> {
        let mut len = 0;
        let tokens = tokens
            .into_iter()
            .map(|t| {
                let t = t.into();
                let next = len + 1 + t.len();
                let pos = std::mem::replace(&mut len, next);
                (pos, t)
            })
            .collect::<Vec<_>>();
        if tokens.is_empty() {
            None
        } else {
            Some(EntryValue::Text(tokens))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serdes_json() {
        let values: EntryValues = vec![
            None,
            Some(true.into()),
            Some(2.into()),
            Some("3".into()),
            EntryValue::text(["4"]),
        ];
        let json = crate::serialization::SerDes::Json
            .serialize(&values)
            .expect("json");
        let json = std::str::from_utf8(&json).expect("utf-8");
        assert_eq!(json, "[null,true,2,\"3\",[[0,\"4\"]]]");
        assert_eq!(
            crate::serialization::SerDes::Json
                .deserialize::<EntryValues>(json.as_bytes())
                .expect("deser"),
            values
        )
    }

    #[test]
    fn serdes_cbor() {
        let values: EntryValues = vec![
            None,
            Some(true.into()),
            Some(false.into()),
            Some(1.into()),
            Some(0.into()),
            Some("3".into()),
            EntryValue::text(["ž"]),
        ];
        let cbor = crate::serialization::SerDes::Cbor
            .serialize(&values)
            .expect("cbor");
        assert_eq!(
            cbor,
            vec![135, 246, 245, 244, 1, 0, 97, 51, 129, 130, 0, 98, 197, 190]
        );
        assert_eq!(
            crate::serialization::SerDes::Cbor
                .deserialize::<EntryValues>(&cbor)
                .expect("deser"),
            values
        )
    }
}
