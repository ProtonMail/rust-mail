mod centered_throbber;
mod conversations;
mod labels;
pub mod messages;
mod scrollable_list;
mod scrollable_paragraph;
mod scrollable_table;
mod text_input;
pub mod utils;

pub use centered_throbber::*;
use ratatui::widgets::{List, Table};
pub use scrollable_list::*;
pub use scrollable_paragraph::*;
pub use scrollable_table::*;
pub use text_input::*;

/// Utility trait to convert items into a table.
pub trait AsTable {
    fn as_table(&self) -> Table<'_>;
}

/// Utility trait to convert items into a list.
pub trait AsList {
    fn as_list(&self) -> List<'_>;
}
