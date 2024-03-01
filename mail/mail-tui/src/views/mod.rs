mod error_dialog;
mod login;
mod mailbox;
mod sessions;
mod totp;

use crate::app::AppContext;
use crate::events::AppEvent;
use crate::state::AppState;
pub use error_dialog::*;
pub use login::*;
pub use mailbox::*;
pub use sessions::*;
pub use totp::*;

pub type AppViewContext = AppContext<AppState, AppEvent>;
