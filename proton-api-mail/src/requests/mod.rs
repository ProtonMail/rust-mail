//! Representation of all the JSON data types that need to be submitted.

mod addresses;
mod conversations;
mod image_proxy;
mod labels;
mod messages;
mod settings;

pub use addresses::*;
pub use conversations::*;
pub use image_proxy::*;
pub use labels::*;
pub use messages::*;
pub use settings::*;
