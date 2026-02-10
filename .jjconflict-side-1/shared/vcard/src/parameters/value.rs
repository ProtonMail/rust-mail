use crate::ParameterType;
use crate::errors::{VCardParameterError, VCardParameterResult};

/// All kind of values than can be used in vCards
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ValueType {
    /// Component value (internally used)
    Component,
    /// Date
    Date,
    /// `Date` or `DateTime` or `Time`
    DateAndOrTime,
    /// Datetime
    DateTime,
    /// Iana token
    IanaToken,
    /// Language Tag
    LanguageTag,
    /// List of components
    ListComponent,
    /// A param-value type (internally used)
    ParamValue,
    /// Text
    Text,
    /// List of texts
    TextList,
    /// Time
    Time,
    /// Timestamp
    Timestamp,
    /// Time zone
    TimeZone,
    /// URI
    Uri,
    /// Offset to UTC time zone
    UTCOffset,
    /// Additional name (starting with 'x-')
    XName,
}

impl PartialEq<String> for ValueType {
    fn eq(&self, other: &String) -> bool {
        let other = other.to_ascii_lowercase();
        match self {
            Self::Component => other == "component",
            Self::Date => other == "date",
            Self::DateAndOrTime => other == "date-and-or-time",
            Self::DateTime => other == "datetime",
            Self::IanaToken => other == "iana-token",
            Self::LanguageTag => other == "language-tag",
            Self::ListComponent => other == "list-component",
            Self::ParamValue => other == "param-value",
            Self::Text => other == "text",
            Self::TextList => other == "text-list",
            Self::Time => other == "time",
            Self::Timestamp => other == "timestamp",
            Self::Uri => other == "uri",
            Self::UTCOffset => other == "utc-offset",
            Self::XName => other == "x-name",
            Self::TimeZone => other == "zone",
        }
    }
}

impl TryFrom<&[String]> for ValueType {
    type Error = VCardParameterError;

    fn try_from(values: &[String]) -> VCardParameterResult<Self> {
        if values.len() != 1 {
            return Err(VCardParameterError::ExpectedExactlyOneValue(
                ParameterType::Value,
                values.to_vec(),
            ));
        }
        Self::try_from(values[0].as_str())
    }
}

impl TryFrom<&str> for ValueType {
    type Error = VCardParameterError;

    fn try_from(value: &str) -> VCardParameterResult<Self> {
        match value.to_ascii_lowercase().as_str() {
            "date-and-or-time" => Ok(Self::DateAndOrTime),
            "iana-token" => Ok(Self::IanaToken),
            "language-tag" => Ok(Self::LanguageTag),
            "list-component" => Ok(Self::ListComponent),
            "text" => Ok(Self::Text),
            "text-list" => Ok(Self::TextList),
            "timestamp" => Ok(Self::Timestamp),
            "uri" => Ok(Self::Uri),
            "utc-offset" => Ok(Self::UTCOffset),
            name if name.starts_with("x-") => Ok(Self::XName),
            name => Err(VCardParameterError::InvalidValue(
                ParameterType::Value,
                name.to_owned(),
            )),
        }
    }
}
