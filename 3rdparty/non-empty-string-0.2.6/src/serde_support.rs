use std::fmt;

use serde::{
    de::{self, Unexpected, Visitor},
    ser::Serialize,
};

use crate::NonEmptyString;

impl Serialize for NonEmptyString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.get())
    }
}

struct NonEmptyStringVisitor;

impl<'de> de::Deserialize<'de> for NonEmptyString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_string(NonEmptyStringVisitor)
    }
}

impl<'de> Visitor<'de> for NonEmptyStringVisitor {
    type Value = NonEmptyString;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string with a length of more than 0")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_string(value.to_owned())
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        NonEmptyString::new(value).map_err(|e| de::Error::invalid_value(Unexpected::Str(&e), &self))
    }
}

#[cfg(test)]
mod tests {
    use crate::*;
    use assert_matches::assert_matches;
    use serde_json::json;

    #[test]
    fn serialize_non_empty_string_and_normal_string_give_the_same_result() {
        let value = NonEmptyString("abc".to_owned());
        let result = serde_json::to_string(&value);

        assert!(result.is_ok());

        let json = serde_json::to_string(&json!("abc")).unwrap();
        assert_eq!(result.unwrap(), json)
    }

    #[test]
    fn deserialize_works() {
        let e: Result<NonEmptyString, _> = serde_json::from_value(json!("abc"));

        let expected = NonEmptyString("abc".to_owned());

        assert_matches!(e, Ok(v) if v == expected)
    }

    fn deserialize_x_fails(value: serde_json::Value, expected_error_message: &'static str) {
        let e: Result<NonEmptyString, _> = serde_json::from_value(value);
        assert_matches!(e, Err(error) if &error.to_string() == expected_error_message)
    }

    #[test]
    fn deserialize_empty_fails() {
        deserialize_x_fails(
            json!(""),
            "invalid value: string \"\", expected a string with a length of more than 0",
        )
    }

    #[test]
    fn deserialize_number_fails() {
        deserialize_x_fails(
            json!(8),
            "invalid type: integer `8`, expected a string with a length of more than 0",
        )
    }

    #[test]
    fn deserialize_object_fails() {
        deserialize_x_fails(
            json!({}),
            "invalid type: map, expected a string with a length of more than 0",
        )
    }

    #[test]
    fn deserialize_sequence_fails() {
        deserialize_x_fails(
            json!([]),
            "invalid type: sequence, expected a string with a length of more than 0",
        )
    }
}
