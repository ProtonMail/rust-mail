//! All the errors that may happen in Muon and that can be faced by a user.

mod types;
pub use crate::app::{
    ParseAppNameErr, ParseAppVersionErr, ParsePlatformErr, ParseProductErr, ParseSemVerErr,
};
pub use crate::env::ParseEnvIdErr;
pub use crate::http::StatusErr;
pub use crate::tls::ParseCertErr;
pub use crate::util::ByteSliceErr;
pub use types::{Error, ErrorKind, Result};

/// Collection of utilities related to errors
pub mod util {
    pub use crate::util::{BoxErrExt, BoxErrFut, BoxErrIntoFut, BoxMapErrFut, FutBoxErrExt};
}
