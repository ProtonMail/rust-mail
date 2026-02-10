use super::*;
use crate::index::text::inner::TextIndex;
use crate::index::text::inner::filter::TextFilter;

/// Trait for text filtering operations that can be performed on a text index
///
/// This trait provides a common interface for different types of text search filters,
/// allowing for flexible and extensible search functionality.
pub trait TextFilterSansIo {
    /// Performs a search operation on the given text index
    ///
    /// # Arguments
    ///
    /// * `index` - The text index to search in
    ///
    /// # Returns
    ///
    /// An iterator over search events with results
    fn get(&self, index: &TextIndex) -> impl Iterator<Item = IndexSearchEvent>;

    /// Returns the attribute index to filter by, if any
    ///
    /// # Returns
    ///
    /// Some(AttributeIndex) if filtering by a specific attribute, None otherwise
    fn attribute(&self) -> Option<AttributeIndex>;
}

/// Search filter representing the text IndexSearch call in one struct
#[derive(Debug, Clone, PartialEq)]
pub struct TextSearch {
    /// textual filter
    pub filter: TextFilter,
    /// either search one or all attributes
    pub attribute: Option<AttributeIndex>,
}

impl TextFilterSansIo for TextFilter {
    fn get(&self, index: &TextIndex) -> impl Iterator<Item = IndexSearchEvent> {
        let (results, stats) = index.search(self, None, None);
        results
            .into_iter()
            .map(|(entry, terms)| IndexSearchEvent::Found(entry, terms))
            .chain(std::iter::once(IndexSearchEvent::Stats(stats)))
    }

    fn attribute(&self) -> Option<AttributeIndex> {
        None
    }
}

impl TextFilterSansIo for TextSearch {
    fn get(&self, index: &TextIndex) -> impl Iterator<Item = IndexSearchEvent> {
        let Self { filter, attribute } = self;
        let (results, stats) = index.search(filter, *attribute, None);

        results
            .into_iter()
            .map(|(entry, terms)| IndexSearchEvent::Found(entry, terms))
            .chain(std::iter::once(IndexSearchEvent::Stats(stats)))
    }

    fn attribute(&self) -> Option<AttributeIndex> {
        self.attribute
    }
}
