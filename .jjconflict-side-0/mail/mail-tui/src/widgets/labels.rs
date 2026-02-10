use crate::widgets::AsList;
use proton_core_common::models::Label;
use ratatui::widgets::{List, ListItem};

impl AsList for Vec<Label> {
    fn as_list(&self) -> List<'_> {
        List::new(self.iter().map(|label| {
            let name = label.path.as_deref().unwrap_or(label.name.as_str());
            ListItem::from(name)
        }))
    }
}
