use tracing::trace;

use super::*;
use crate::index::prelude::*;

pub struct Negation {
    /// Search to be negated, assuming that results are sorted by EntryIndex
    search: Box<dyn Send + Iterator<Item = ExpressionSearchEvent>>,
    entries: VecDeque<EntryIndex>,
    skip: Option<EntryIndex>,
}

impl Negation {
    pub fn new(
        search: Box<dyn Send + Iterator<Item = ExpressionSearchEvent>>,
        entries: &dyn Fn() -> VecDeque<EntryIndex>,
    ) -> Self {
        let entries: VecDeque<EntryIndex> = entries();
        debug_assert!(entries.iter().is_sorted());
        Self {
            search,
            entries,
            skip: None,
        }
    }
}

impl Iterator for Negation {
    type Item = ExpressionSearchEvent;

    #[instrument]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some((skip, front)) = self.skip.zip(self.entries.front()) {
                trace!("skip {skip:?} vs front {front:?}");

                if *front < skip {
                    break self
                        .entries
                        .pop_front()
                        .map(|e| ExpressionSearchEvent::Found(e, MatchNode::empty()));
                } else if *front == skip {
                    self.entries.pop_front();
                }
            }

            break match self.search.next() {
                None => {
                    // search completed, dump the rest
                    self.entries
                        .pop_front()
                        .map(|e| ExpressionSearchEvent::Found(e, MatchNode::empty()))
                }
                Some(ExpressionSearchEvent::Found(skip, _)) => {
                    trace!(?skip);
                    self.skip = Some(skip);
                    continue;
                }
                other => other,
            };
        }
    }
}

impl Debug for Negation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            search: _,
            entries,
            skip,
        } = self;
        f.debug_struct("Negation")
            .field("entries", &entries)
            .field("skip", &skip)
            .finish_non_exhaustive()
    }
}
