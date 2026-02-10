mod backdrop;
mod centered_throbber;
mod conversations;
mod labels;
pub mod lock_icon;
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
use ratatui::layout::Constraint;
use ratatui::style::Style;
use ratatui::style::Stylize as _;
use ratatui::widgets::Row;
use ratatui::widgets::{List, Table};

pub trait AsIntoTable {
    fn as_table(&self) -> IntoTable<'_>;
}

pub trait AsList {
    fn as_list(&self) -> List<'_>;
}

pub struct IntoTable<'a> {
    pub header: Row<'a>,
    pub rows: Vec<Row<'a>>,
    pub widths: Vec<Constraint>,
}

impl<'a> IntoTable<'a> {
    pub fn new<R, C>(rows: R, widths: C, header: impl Into<Row<'a>>) -> Self
    where
        R: IntoIterator,
        R::Item: Into<Row<'a>>,
        C: IntoIterator,
        C::Item: Into<Constraint>,
    {
        Self {
            rows: rows.into_iter().map(Into::into).collect(),
            header: header.into(),
            widths: widths.into_iter().map(Into::into).collect(),
        }
    }

    fn into_table(self) -> Table<'a> {
        Table::new(self.rows, self.widths)
            .column_spacing(1)
            .header(self.header)
            .highlight_style(Style::new().reversed())
    }
}
