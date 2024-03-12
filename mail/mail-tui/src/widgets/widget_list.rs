#![allow(unused)]
/// Patched from Ratatui List
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::{StatefulWidget, Style, Styled, Widget};
use unicode_width::UnicodeWidthStr;

use ratatui::widgets::ListDirection;
use ratatui::{
    prelude::*,
    widgets::{Block, HighlightSpacing},
};

pub trait ListableWidget: Widget {
    fn height(&self) -> u16;
}
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash)]
pub struct WidgetListState {
    offset: usize,
    selected: Option<usize>,
}

impl WidgetListState {
    #[allow(unused)]
    #[must_use = "method moves the value of self and returns the modified value"]
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    #[allow(unused)]
    #[must_use = "method moves the value of self and returns the modified value"]
    pub fn with_selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn offset_mut(&mut self) -> &mut usize {
        &mut self.offset
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn selected_mut(&mut self) -> &mut Option<usize> {
        &mut self.selected
    }

    pub fn select(&mut self, index: Option<usize>) {
        self.selected = index;
        if index.is_none() {
            self.offset = 0;
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct WidgetListItem<T: ListableWidget> {
    content: T,
    style: Style,
}

impl<T: ListableWidget> WidgetListItem<T> {
    pub fn new(content: T) -> Self {
        Self {
            content,
            style: Style::default(),
        }
    }
    #[must_use = "method moves the value of self and returns the modified value"]
    pub fn style<S: Into<Style>>(mut self, style: S) -> Self {
        self.style = style.into();
        self
    }

    pub fn height(&self) -> u16 {
        self.content.height()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Default)]
pub struct WidgetList<'a, T: ListableWidget> {
    block: Option<Block<'a>>,
    items: Vec<WidgetListItem<T>>,
    /// Style used as a base style for the widget
    style: Style,
    /// List display direction
    direction: ListDirection,
    /// Style used to render selected item
    highlight_style: Style,
    /// Symbol in front of the selected item (Shift all items to the right)
    highlight_symbol: Option<&'a str>,
    /// Whether to repeat the highlight symbol for each line of the selected item
    repeat_highlight_symbol: bool,
    /// Decides when to allocate spacing for the selection symbol
    highlight_spacing: HighlightSpacing,
}

impl<'a, T: ListableWidget> WidgetList<'a, T> {
    pub fn new(items: impl IntoIterator<Item = WidgetListItem<T>>) -> Self {
        Self {
            block: None,
            style: Style::default(),
            items: items.into_iter().collect(),
            direction: ListDirection::default(),
            highlight_style: Default::default(),
            highlight_symbol: None,
            repeat_highlight_symbol: false,
            highlight_spacing: Default::default(),
        }
    }

    /// Set the items
    pub fn items<I>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = WidgetListItem<T>>,
    {
        self.items = items.into_iter().collect();
        self
    }

    #[must_use = "method moves the value of self and returns the modified value"]
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    #[must_use = "method moves the value of self and returns the modified value"]
    pub fn style<S: Into<Style>>(mut self, style: S) -> Self {
        self.style = style.into();
        self
    }
    #[must_use = "method moves the value of self and returns the modified value"]
    pub fn highlight_symbol(mut self, highlight_symbol: &'a str) -> Self {
        self.highlight_symbol = Some(highlight_symbol);
        self
    }

    #[must_use = "method moves the value of self and returns the modified value"]
    pub fn highlight_style<S: Into<Style>>(mut self, style: S) -> Self {
        self.highlight_style = style.into();
        self
    }
    #[must_use = "method moves the value of self and returns the modified value"]
    pub fn repeat_highlight_symbol(mut self, repeat: bool) -> Self {
        self.repeat_highlight_symbol = repeat;
        self
    }

    #[must_use = "method moves the value of self and returns the modified value"]
    pub fn highlight_spacing(mut self, value: HighlightSpacing) -> Self {
        self.highlight_spacing = value;
        self
    }

    #[must_use = "method moves the value of self and returns the modified value"]
    pub fn direction(mut self, direction: ListDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Returns the number of [`ratatui::widgets::ListItem`]s in the list
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns true if the list contains no elements.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Given an offset, calculate which items can fit in a given area
    fn get_items_bounds(
        &self,
        selected: Option<usize>,
        offset: usize,
        max_height: usize,
    ) -> (usize, usize) {
        let offset = offset.min(self.items.len().saturating_sub(1));

        // Note: visible here implies visible in the given area
        let mut first_visible_index = offset;
        let mut last_visible_index = offset;

        // Current height of all items in the list to render, beginning at the offset
        let mut height_from_offset = 0;

        // Calculate the last visible index and total height of the items
        // that will fit in the available space
        for item in self.items.iter().skip(offset) {
            if height_from_offset + item.height() as usize > max_height {
                break;
            }

            height_from_offset += item.height() as usize;

            last_visible_index += 1;
        }

        // Get the selected index, but still honor the offset if nothing is selected
        // This allows for the list to stay at a position after select()ing None.
        let index_to_display = selected.unwrap_or(offset).min(self.items.len() - 1);

        // Recall that last_visible_index is the index of what we
        // can render up to in the given space after the offset
        // If we have an item selected that is out of the viewable area (or
        // the offset is still set), we still need to show this item
        while index_to_display >= last_visible_index {
            height_from_offset =
                height_from_offset.saturating_add(self.items[last_visible_index].height() as usize);

            last_visible_index += 1;

            // Now we need to hide previous items since we didn't have space
            // for the selected/offset item
            while height_from_offset > max_height {
                height_from_offset = height_from_offset
                    .saturating_sub(self.items[first_visible_index].height() as usize);

                // Remove this item to view by starting at the next item index
                first_visible_index += 1;
            }
        }

        // Here we're doing something similar to what we just did above
        // If the selected item index is not in the viewable area, let's try to show the item
        while index_to_display < first_visible_index {
            first_visible_index -= 1;

            height_from_offset = height_from_offset
                .saturating_add(self.items[first_visible_index].height() as usize);

            // Don't show an item if it is beyond our viewable height
            while height_from_offset > max_height {
                last_visible_index -= 1;

                height_from_offset = height_from_offset
                    .saturating_sub(self.items[last_visible_index].height() as usize);
            }
        }

        (first_visible_index, last_visible_index)
    }
}

impl<T: ListableWidget> Widget for WidgetList<'_, T> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut state = WidgetListState::default();
        self.render_ref(area, buf, &mut state)
    }
}

impl<T: ListableWidget> StatefulWidget for WidgetList<'_, T> {
    type State = WidgetListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        self.render_ref(area, buf, state);
    }
}

fn should_add(hls: &HighlightSpacing, has_selection: bool) -> bool {
    match hls {
        HighlightSpacing::Always => true,
        HighlightSpacing::WhenSelected => has_selection,
        HighlightSpacing::Never => false,
    }
}

impl<'a, T: ListableWidget> WidgetList<'a, T> {
    fn render_ref(self, area: Rect, buf: &mut Buffer, state: &mut WidgetListState) {
        buf.set_style(area, self.style);
        self.block.render(area, buf);
        let list_area = self.block.inner_if_some(area);

        if list_area.is_empty() || self.items.is_empty() {
            return;
        }

        let list_height = list_area.height as usize;

        let (first_visible_index, last_visible_index) =
            self.get_items_bounds(state.selected, state.offset, list_height);

        // Important: this changes the state's offset to be the beginning of the now viewable items
        state.offset = first_visible_index;

        // Get our set highlighted symbol (if one was set)
        let highlight_symbol = self.highlight_symbol.unwrap_or("");
        let blank_symbol = " ".repeat(highlight_symbol.width());

        let mut current_height = 0;
        let selection_spacing = should_add(&self.highlight_spacing, state.selected.is_some());
        for (i, item) in self
            .items
            .into_iter()
            .enumerate()
            .skip(state.offset)
            .take(last_visible_index - first_visible_index)
        {
            let (x, y) = if self.direction == ratatui::widgets::ListDirection::BottomToTop {
                current_height += item.height();
                (list_area.left(), list_area.bottom() - current_height)
            } else {
                let pos = (list_area.left(), list_area.top() + current_height);
                current_height += item.height();
                pos
            };

            let row_area = Rect {
                x,
                y,
                width: list_area.width,
                height: item.height(),
            };

            let item_style = self.style.patch(item.style);
            buf.set_style(row_area, item_style);

            let is_selected = state.selected.map_or(false, |s| s == i);

            let item_area = if selection_spacing {
                let highlight_symbol_width =
                    u16::try_from(self.highlight_symbol.unwrap_or("").width()).unwrap();
                Rect {
                    x: row_area.x + highlight_symbol_width,
                    width: row_area.width - highlight_symbol_width,
                    ..row_area
                }
            } else {
                row_area
            };

            let item_height = item.content.height();
            item.content.render(item_area, buf);

            for j in 0..item_height {
                // if the item is selected, we need to display the highlight symbol:
                // - either for the first line of the item only, 
                // - or for each line of the item if the appropriate option is set
                let symbol = if is_selected && (j == 0 || self.repeat_highlight_symbol) {
                    highlight_symbol
                } else {
                    &blank_symbol
                };
                if selection_spacing {
                    buf.set_stringn(x, y + j, symbol, list_area.width as usize, item_style);
                }
            }

            if is_selected {
                buf.set_style(row_area, self.highlight_style);
            }
        }
    }
}

impl<'a, T: ListableWidget> Styled for WidgetList<'a, T> {
    type Item = WidgetList<'a, T>;

    fn style(&self) -> Style {
        self.style
    }

    fn set_style<S: Into<Style>>(self, style: S) -> Self::Item {
        self.style(style)
    }
}

impl<T: ListableWidget> Styled for WidgetListItem<T> {
    type Item = WidgetListItem<T>;

    fn style(&self) -> Style {
        self.style
    }

    fn set_style<S: Into<Style>>(self, style: S) -> Self::Item {
        self.style(style)
    }
}
