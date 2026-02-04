use serde::de::{SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize};
use tracing::warn;

use crate::index::prelude::{AttributeIndex, EntryIndex, TokenPosition, ValueIndex};
use crate::index::text::text_with_buckets::tokens::Tokens;
use crate::index::text::text_with_buckets::vocabulary::TokenRef;

impl Serialize for Tokens {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(None)?;
        for (tref, occurrences) in self.iter() {
            seq.serialize_element(&TokenField::TOKEN)?;
            seq.serialize_element(tref)?;
            let mut last_attr = None;
            let mut last_entry = None;
            let mut last_value = None;
            let mut last_position = None;
            for (a, v, e, p) in occurrences {
                let last_attr = last_attr.replace(*a);
                let last_entry = last_entry.replace(*e);
                let last_value = last_value.replace(*v);
                let last_position = last_position.replace(*p);
                let mut rollover = false;
                if last_attr != Some(*a) {
                    seq.serialize_element(&TokenField::ATTR)?;
                    seq.serialize_element(&a)?;
                    rollover = true;
                }
                if rollover || last_value != Some(*v) {
                    seq.serialize_element(&TokenField::VALUE)?;
                    seq.serialize_element(&v)?;
                    rollover = true;
                }
                if rollover || last_entry != Some(*e) {
                    seq.serialize_element(&TokenField::ENTRY)?;
                    seq.serialize_element(&e)?;
                    rollover = true;
                }
                let position_delta = if rollover {
                    p.0
                } else {
                    p.0 - last_position.map(|p| p.0).unwrap_or_default()
                };

                seq.serialize_element(&TokenField::value(position_delta as u64))?;
            }
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for Tokens {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(TokensVisitor::default())
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
enum TokenField {
    Value(u64),
    Marker(i8),
}

impl TokenField {
    const TOKEN: Self = TokenField::Marker(-1);
    const ATTR: Self = TokenField::Marker(-2);
    const VALUE: Self = TokenField::Marker(-3);
    const ENTRY: Self = TokenField::Marker(-4);
    fn value(v: u64) -> Self {
        TokenField::Value(v)
    }
}

#[derive(Debug, Default)]
struct TokensVisitor {}

impl<'de> Visitor<'de> for TokensVisitor {
    /// Return type of this visitor. This visitor computes the max of a
    /// sequence of values of type T, so the type of the maximum is T.
    type Value = Tokens;

    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
        formatter.write_str("a nonempty sequence of numbers")
    }

    fn visit_seq<S>(self, mut seq: S) -> Result<Tokens, S::Error>
    where
        S: SeqAccess<'de>,
    {
        let mut tokens = Tokens::default();
        let mut occurrences = None;
        let mut last_token = None;
        let mut last_attr = None;
        let mut last_entry = None;
        let mut last_value = None;
        let mut last_position: Option<u64> = None;
        while let Some(field) = seq.next_element::<TokenField>()? {
            match field {
                TokenField::Marker(_) if field == TokenField::TOKEN => {
                    last_token = seq.next_element::<TokenRef>()?;
                    occurrences = last_token.map(|t| tokens.entry(t).or_default());
                    last_attr = None;
                    last_entry = None;
                    last_value = None;
                    last_position = None;
                }
                TokenField::Marker(_) if field == TokenField::ATTR => {
                    last_attr = seq.next_element::<AttributeIndex>()?;
                    last_entry = None;
                    last_value = None;
                    last_position = None;
                }
                TokenField::Marker(_) if field == TokenField::VALUE => {
                    if last_position.is_none() && last_attr.is_some() {
                        warn!(
                            "Bad value input, got attr {last_token:?} {last_attr:?} but no position"
                        )
                    }
                    last_value = seq.next_element::<ValueIndex>()?;
                    last_entry = None;
                    last_position = None;
                }
                TokenField::Marker(_) if field == TokenField::ENTRY => {
                    if last_position.is_none() && last_value.is_some() {
                        warn!(
                            "Bad entry input, got attr {last_token:?} {last_attr:?} {last_value:?} but no position"
                        )
                    }
                    last_entry = seq.next_element::<EntryIndex>()?;
                    last_position = None;
                }
                TokenField::Value(position_delta) => {
                    let ((((occurrences,_), a), e), v) =
                        occurrences.as_mut().zip(last_token).zip(last_attr).zip(last_entry).zip(last_value).ok_or_else(||
                            serde::de::Error::custom(format!(
                            "Value out of place: {last_token:?} {last_attr:?} {last_entry:?} {last_value:?}"
                        )))?;

                    let position = position_delta + last_position.unwrap_or_default();
                    last_position = Some(position);

                    occurrences.insert((a, v, e, TokenPosition(position as usize)));
                }
                _ => {
                    return Err(serde::de::Error::custom(format!(
                        "Unrecognized field {field:?}. {last_token:?} {last_attr:?} {last_entry:?} {last_value:?}"
                    )));
                }
            }
        }

        let complete_record = last_position.is_some()
            || last_attr.is_none() && last_entry.is_none() && last_value.is_none();

        if !complete_record {
            warn!(
                "Bad input: no position for {last_token:?} {last_attr:?} {last_entry:?} {last_value:?}"
            )
        }

        Ok(tokens)
    }
}

#[test]
fn serde_tokens() {
    let tokens = fixture();

    let json = serde_json::to_string_pretty(&tokens).expect("json");

    insta::assert_snapshot!(json);

    let tokens_roundtrip: Tokens = serde_json::from_str(&json).expect("tokens");

    assert_eq!(tokens_roundtrip, tokens);

    fn fixture() -> Tokens {
        let data = [
            // empty list is not occurring, but valid
            (TokenRef(0), [].into()),
            (
                // alterations in A->E
                TokenRef(1),
                [
                    (
                        AttributeIndex(0),
                        ValueIndex(0),
                        EntryIndex(0),
                        TokenPosition(0),
                    ),
                    (
                        AttributeIndex(1),
                        ValueIndex(0),
                        EntryIndex(0),
                        TokenPosition(1),
                    ),
                    (
                        AttributeIndex(1),
                        ValueIndex(0),
                        EntryIndex(0),
                        TokenPosition(2),
                    ),
                    (
                        AttributeIndex(1),
                        ValueIndex(0),
                        EntryIndex(1),
                        TokenPosition(2),
                    ),
                ]
                .into(),
            ),
            (
                // alterations in E->V
                TokenRef(10),
                [
                    (
                        AttributeIndex(0),
                        ValueIndex(0),
                        EntryIndex(0),
                        TokenPosition(0),
                    ),
                    (
                        AttributeIndex(0),
                        ValueIndex(0),
                        EntryIndex(1),
                        TokenPosition(10),
                    ),
                    (
                        AttributeIndex(0),
                        ValueIndex(0),
                        EntryIndex(1),
                        TokenPosition(20),
                    ),
                    (
                        AttributeIndex(0),
                        ValueIndex(1),
                        EntryIndex(1),
                        TokenPosition(20),
                    ),
                ]
                .into(),
            ),
            (
                // alterations in V
                TokenRef(100),
                [
                    (
                        AttributeIndex(0),
                        ValueIndex(0),
                        EntryIndex(0),
                        TokenPosition(0),
                    ),
                    (
                        AttributeIndex(0),
                        ValueIndex(1),
                        EntryIndex(0),
                        TokenPosition(100),
                    ),
                    (
                        AttributeIndex(0),
                        ValueIndex(1),
                        EntryIndex(0),
                        TokenPosition(200),
                    ),
                    (
                        AttributeIndex(0),
                        ValueIndex(2),
                        EntryIndex(0),
                        TokenPosition(300),
                    ),
                ]
                .into(),
            ),
        ];
        let mut tokens = Tokens::default();
        tokens.extend(data);
        tokens
    }
}
