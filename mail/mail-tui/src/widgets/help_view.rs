use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::prelude::{Margin, StatefulWidget, Stylize};
use ratatui::text::Text;
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

pub struct HelpItem {
    pub key: &'static str,
    pub description: &'static str,
}

pub struct HelpCategory {
    pub name: &'static str,
    pub items: &'static [HelpItem],
}

#[derive(Default)]
pub struct HelpViewState {}

pub struct HelpView<'a> {
    categories: &'a [HelpCategory],
}

impl<'a> HelpView<'a> {
    pub fn new(categories: &'a [HelpCategory]) -> Self {
        Self { categories }
    }

    fn largest_help_item_size(&self) -> u16 {
        let mut largest_item_size = 0_u16;
        for category in self.categories {
            for item in category.items {
                //TODO: UTF8 to char length.
                largest_item_size = largest_item_size
                    .max(u16::try_from(item.key.len()).expect("exceeds max value"));
            }
        }
        // +1 for padding
        largest_item_size + 1
    }

    // TODO: tui_scroll_view crate is not very complete.
    /*
    fn total_height(&self) -> u16 {
        let mut total = self.categories.len() * 2;
        for c in self.categories {
            total += c.items.len();
        }
        u16::try_from(total).expect("exceeds max value")
    }
     */
}

impl<'a> StatefulWidget for HelpView<'a> {
    type State = HelpViewState;

    fn render(self, area: Rect, buf: &mut Buffer, _: &mut Self::State) {
        if self.categories.is_empty() {
            return;
        };

        let area = area.inner(&Margin {
            horizontal: 2,
            vertical: 2,
        });

        let [area, _] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)]).areas(area);
        let largest_item = self.largest_help_item_size();

        let constraints = self
            .categories
            .iter()
            .map(|c| {
                Constraint::Length(u16::try_from(c.items.len() + 2).expect("exceeds max range"))
            })
            .collect::<Vec<_>>();
        let areas = Layout::vertical(constraints).split(area);

        for (index, category) in self.categories.iter().enumerate() {
            let block_area = areas[index];
            Block::new()
                .borders(Borders::TOP)
                .title(category.name)
                .render(block_area, buf);

            let content_area = block_area.inner(&Margin {
                horizontal: 1,
                vertical: 1,
            });
            let line_areas = Layout::vertical(
                std::iter::repeat(Constraint::Length(1))
                    .take(category.items.len())
                    .collect::<Vec<_>>(),
            )
            .split(content_area);
            for (item_index, item) in category.items.iter().enumerate() {
                let [key_area, desc_area] =
                    Layout::horizontal([Constraint::Length(largest_item), Constraint::Fill(10)])
                        .areas(line_areas[item_index]);
                Text::from(item.key).bold().render(key_area, buf);
                Paragraph::new(item.description)
                    .wrap(Wrap { trim: true })
                    .render(desc_area, buf);
            }
        }
    }
}
