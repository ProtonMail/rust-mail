use std::collections::HashSet;

use crate::ParameterType;
use crate::errors::{VCardParameterError, VCardParameterResult};

// /// The TYPE parameter has multiple, different uses.  In general, it is a way of specifying class
// /// characteristics of the associated property.
//  related-type-value = "contact" / "acquaintance" / "friend" / "met"
//                     / "co-worker" / "colleague" / "co-resident"
//                     / "neighbor" / "child" / "parent"
//                     / "sibling" / "spouse" / "kin" / "muse"
//                     / "crush" / "date" / "sweetheart" / "me"
//                     / "agent" / "emergency"
pub(super) const RELATED_VALUES: [&str; 20] = [
    "contact",
    "acquaintance",
    "friend",
    "met",
    "co-worker",
    "colleague",
    "co-resident",
    "neighbor",
    "child",
    "parent",
    "sibling",
    "spouse",
    "kin",
    "muse",
    "crush",
    "date",
    "sweetheart",
    "me",
    "agent",
    "emergency",
];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RelatedType {
    Home,
    Work,
    Contact,
    Acquaintance,
    Friend,
    Met,
    CoWorker,
    Colleague,
    CoResident,
    Neighbor,
    Child,
    Parent,
    Sibling,
    Spouse,
    Kin,
    Muse,
    Crush,
    Date,
    Sweetheart,
    Me,
    Agent,
    Emergency,
}

impl RelatedType {
    /// Try to create a new TYPE parameter for Related property
    pub fn new_validated(value: &str) -> VCardParameterResult<Self> {
        Self::try_from(value)
    }

    /// Try to create a new `HashSet` of TYPE parameters
    pub fn set_from_values(values: &[String]) -> VCardParameterResult<HashSet<Self>> {
        values
            .iter()
            .map(|v| TryInto::try_into(v.as_str()))
            .collect()
    }
}

impl TryFrom<&str> for RelatedType {
    type Error = VCardParameterError;

    fn try_from(value: &str) -> VCardParameterResult<Self> {
        match value.to_ascii_lowercase().as_ref() {
            "home" => Ok(Self::Home),
            "work" => Ok(Self::Work),
            "contact" => Ok(Self::Contact),
            "acquaintance" => Ok(Self::Acquaintance),
            "friend" => Ok(Self::Friend),
            "met" => Ok(Self::Met),
            "co-worker" => Ok(Self::CoWorker),
            "colleague" => Ok(Self::Colleague),
            "co-resident" => Ok(Self::CoResident),
            "neighbor" => Ok(Self::Neighbor),
            "child" => Ok(Self::Child),
            "parent" => Ok(Self::Parent),
            "sibling" => Ok(Self::Sibling),
            "spouse" => Ok(Self::Spouse),
            "kin" => Ok(Self::Kin),
            "muse" => Ok(Self::Muse),
            "crush" => Ok(Self::Crush),
            "date" => Ok(Self::Date),
            "sweetheart" => Ok(Self::Sweetheart),
            "me" => Ok(Self::Me),
            "agent" => Ok(Self::Agent),
            "emergency" => Ok(Self::Emergency),
            _ => Err(VCardParameterError::InvalidValue(
                ParameterType::Type,
                value.to_owned(),
            )),
        }
    }
}
