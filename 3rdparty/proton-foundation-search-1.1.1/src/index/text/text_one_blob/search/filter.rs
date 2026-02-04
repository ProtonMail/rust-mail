use super::filter::TextFilter;
use super::inner::TextIndex;
use super::*;

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
    fn get(
        &self,
        index: &TextIndex,
        collect_stats: bool,
        collect_postitions: bool,
    ) -> impl Iterator<Item = IndexSearchEvent>;
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
    fn get(
        &self,
        index: &TextIndex,
        collect_stats: bool,
        collect_postitions: bool,
    ) -> impl Iterator<Item = IndexSearchEvent> {
        let (results, stats) = index.search(self, None, None, collect_stats, collect_postitions);
        results
            .into_iter()
            .map(|(entry, terms)| IndexSearchEvent::Found(entry, terms))
            .chain(stats.map(IndexSearchEvent::Stats))
    }
}

impl TextFilterSansIo for TextSearch {
    fn get(
        &self,
        index: &TextIndex,
        collect_stats: bool,
        collect_postitions: bool,
    ) -> impl Iterator<Item = IndexSearchEvent> {
        let Self { filter, attribute } = self;
        let (results, stats) =
            index.search(filter, *attribute, None, collect_stats, collect_postitions);

        results
            .into_iter()
            .map(|(entry, terms)| IndexSearchEvent::Found(entry, terms))
            .chain(stats.map(IndexSearchEvent::Stats))
    }
}
