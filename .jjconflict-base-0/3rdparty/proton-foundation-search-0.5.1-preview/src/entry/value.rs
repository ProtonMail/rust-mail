use super::*;

/// A list of index values
pub type EntryValues = Vec<EntryValue>;

/// An indexed value contains all values associated with an entry attribute.
///
/// It is passed to each index and each index takes what it need for the given attribute.
///
/// Schema is defined by the app by inserting entry-attribute-values and technically,
/// an attribute can hold values of different types.
///
/// In order to make a round trip between export/import of entries, the atomic entry-attribute value
/// is a list of different [`EntryValue`], preserving the value index of each value.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[repr(C)]
#[serde(untagged)]
pub enum EntryValue {
    /// No particular value
    #[default]
    Empty,
    /// Tokenized text blob values with their original positions
    Text(Vec<(usize, Box<str>)>),
    /// A tag value.
    Tag(Box<str>),
    /// An integer value.
    Integer(u64),
    /// A boolean value.
    Boolean(bool),
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {

    use super::*;

    #[test]
    fn serdes_json() {
        let values: EntryValues = vec![
            EntryValue::Empty,
            true.into(),
            2.into(),
            "3".into(),
            vec![(0, "4".into())].into(),
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
            EntryValue::Empty,
            true.into(),
            false.into(),
            1.into(),
            0.into(),
            "3".into(),
            vec![(0, "ž".into())].into(),
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
