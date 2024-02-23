mod error_dialog;
mod login;
mod mailbox;
mod totp;

use crate::app::AppContext;
use crate::events::AppEvents;
use crate::state::AppState;
pub use error_dialog::*;
pub use login::*;
pub use totp::*;

pub type AppViewContext = AppContext<AppState, AppEvents>;
