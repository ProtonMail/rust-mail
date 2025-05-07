use crate::{VEventViolation, VTimeZoneViolation};
use itertools::Itertools;
use thiserror::Error as TError;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Clone, Debug, PartialEq, Eq, TError)]
pub enum Error {
    #[error("event {0} not found")]
    MissingEvent(usize),

    #[error("time zone {0} not foun")]
    MissingTimeZone(usize),

    #[error("{}", .0.iter().join(" ; "))]
    Violations(Vec<Violation>),
}

impl Error {
    pub fn viol(viols: impl IntoIterator<Item = Violation>) -> Self {
        Error::Violations(viols.into_iter().collect())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, TError)]
pub enum Violation {
    #[error("event[{0}]: {1}")]
    InvalidEvent(usize, VEventViolation),

    #[error("timezone[{0}]: {0}")]
    InvalidTimeZone(usize, VTimeZoneViolation),

    #[error("missing calendar")]
    MissingCalendar,
}
