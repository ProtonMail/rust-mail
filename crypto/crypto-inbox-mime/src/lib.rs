mod constants;

mod read;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

pub use read::*;

pub mod write;

/// Possible dispositions of an attachment in the MIME builder.
/// Either inline or an attachment.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Disposition {
    /// A regular attachment.
    Attachment,

    // An inline/embedded attachment.
    Inline,
}

impl Disposition {
    /// Returns a reference to its string representation.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Disposition::Attachment => "attachment",
            Disposition::Inline => "inline",
        }
    }
}

impl Display for Disposition {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Disposition {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "attachment" => Ok(Disposition::Attachment),
            "inline" => Ok(Disposition::Inline),
            _ => Err(()),
        }
    }
}
