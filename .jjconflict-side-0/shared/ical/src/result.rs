use crate::{ReadMsg, VEventViolation, VTimeZoneViolation};
use itertools::Itertools;
use thiserror::Error as TError;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Clone, Debug, PartialEq, Eq, TError)]
pub enum Error {
    #[error("event {0} not found")]
    MissingEvent(usize),

    #[error("time zone {0} not found")]
    MissingTimeZone(usize),

    #[error("invalid *.ics:\n\n{}", .0.iter().join("\n\n"))]
    InvalidIcs(Vec<ReadMsg>),
}

impl Error {
    #[must_use]
    pub fn as_invalid_ics(&self) -> Option<&[ReadMsg]> {
        if let Error::InvalidIcs(msgs) = self {
            Some(msgs)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, TError)]
pub enum Violation {
    #[error("viol: event[{0}]: {1}")]
    InvalidEvent(usize, VEventViolation),

    #[error("viol: timezone[{0}]: {0}")]
    InvalidTimeZone(usize, VTimeZoneViolation),
}
