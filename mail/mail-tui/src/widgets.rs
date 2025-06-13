mod backdrop;
mod centered_throbber;
mod conversations;
mod labels;
pub mod messages;
mod scrollable_list;
mod scrollable_paragraph;
mod scrollable_table;
mod text_input;
pub mod utils;

pub use self::backdrop::*;
pub use self::centered_throbber::*;
pub use self::scrollable_list::*;
pub use self::scrollable_paragraph::*;
pub use self::scrollable_table::*;
pub use self::text_input::*;
use ratatui::widgets::{List, Table};

pub trait AsTable {
    fn as_table(&self) -> Table<'_>;
}

pub trait AsList {
    fn as_list(&self) -> List<'_>;
}
