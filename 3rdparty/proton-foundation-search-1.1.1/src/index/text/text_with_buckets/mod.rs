mod content;
mod export;
mod search;
mod serialization;
mod tokens;
mod trigrams;
mod vocabulary;
mod write;

use serde::{Deserialize, Serialize};
use tracing::{instrument, trace};

use self::export::Export;
use self::search::Search;
use self::write::Write;
use crate::blob::BlobId;
use crate::index::prelude::*;
use crate::index::text::term::TextTerm;
use crate::prelude::*;
use crate::query::expression::{Func, TermValue};
use crate::query::option::QueryOptions;
use crate::query::option::results::{CollectPositions, CollectStats};
use crate::query::option::text::{MaximumDistance, MinimumSimilarity};

/// The scaled text index implementation with buckets for entry occurrences
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TextIndex {
    /// When maximum bucket size is reached, a new one is created for a transaction
    /// 0 => new bucket for each transaction
    pub maximum_token_bucket_size: usize,
}
impl TextIndex {
    /// Create a new TextIndex
    pub fn new(maximum_token_bucket_size: usize) -> Self {
        Self {
            maximum_token_bucket_size,
        }
    }
}

impl IndexStore for TextIndex {
    fn id(&self) -> &str {
        "text"
    }

    fn write(
        &self,
        revision: Option<BlobId>,
        operations: &[IndexStoreOperation],
    ) -> Box<dyn Send + Iterator<Item = IndexStoreEvent>> {
        let mut write = Write::new(self.clone(), revision);
        for op in operations {
            match op {
                IndexStoreOperation::Insert(entry, attr, values) => {
                    write.insert(*entry, *attr, values.clone())
                }
                IndexStoreOperation::Remove(entry) => write.remove(*entry),
            }
        }
        Box::new(write.commit())
    }
}

impl IndexSearch for TextIndex {
    #[instrument]
    fn search(
        &self,
        revision: BlobId,
        attribute: Option<AttributeIndex>,
        function: Func,
        value: &TermValue,
        options: &QueryOptions,
    ) -> Option<Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>> {
        let collect_stats = CollectStats::get(options);
        let collect_positions = CollectPositions::get(options);
        let mut max_distance = MaximumDistance::get(options);
        let mut min_similarity = MinimumSimilarity::get(options);
        let term = TextTerm::new(value.wildcard_text()?);

        match function {
            Func::Matches => {}
            Func::Equals => {
                // exact search
                max_distance = 0;
                min_similarity = 1.0;
            }
            Func::GreaterThan
            | Func::GreaterThanOrEqual
            | Func::LessThan
            | Func::LessThanOrEqual => return None,
        }

        trace!(
            ?term,
            ?max_distance,
            ?min_similarity,
            ?collect_stats,
            ?collect_positions
        );

        Some(Box::new(Search::new(
            revision,
            attribute,
            term,
            max_distance,
            min_similarity,
            collect_stats,
            collect_positions,
        )))
    }
}

impl IndexExport for TextIndex {
    fn export(
        &self,
        revision: BlobId,
    ) -> Box<dyn 'static + Send + Iterator<Item = IndexExportEvent>> {
        Box::new(Export::new(revision))
    }
}
