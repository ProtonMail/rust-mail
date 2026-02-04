use std::collections::VecDeque;

use tracing::trace;

use super::*;
use crate::index::prelude::*;

#[derive(Debug)]
pub struct Union {
    /// Searches to be merged, assuming that results are sorted by EntryIndex
    searches: Vec<Search>,
    /// Outbound events
    ready: VecDeque<ExpressionSearchEvent>,
}

struct Search {
    search: Box<dyn Send + Iterator<Item = ExpressionSearchEvent>>,
    matched: Option<(EntryIndex, MatchNode<IndexRealm>)>,
}

impl Search {
    fn entry(&self) -> Option<EntryIndex> {
        self.matched.as_ref().map(|(e, _)| *e)
    }
    fn take_matches(&mut self, entry: EntryIndex) -> Option<MatchNode<IndexRealm>> {
        if self.entry()? == entry {
            self.matched.take().map(|(_, m)| m)
        } else {
            None
        }
    }
    fn poll(&mut self) -> Option<ExpressionSearchEvent> {
        if self.matched.is_some() {
            return None;
        }
        let event = self.search.next();
        if let Some(ExpressionSearchEvent::Found(entry, matched)) = event {
            self.matched = Some((entry, matched));
            return None;
        }
        event
    }
}

impl Union {
    pub fn new(
        searches: impl IntoIterator<
            Item = impl Into<Box<dyn Send + Iterator<Item = ExpressionSearchEvent>>>,
        >,
    ) -> Self {
        Self {
            searches: searches
                .into_iter()
                .map(|search| Search {
                    search: search.into(),
                    matched: None,
                })
                .collect(),
            ready: [].into(),
        }
    }

    #[instrument]
    fn complete_lowest(&mut self) -> Option<ExpressionSearchEvent> {
        let mut target = None;
        self.searches.retain(|s| {
            // find the lowest entry, provided all searches have a match
            target = match (s.entry(), target) {
                // competition
                (Some(entry), Some(target)) => Some(target.min(entry)),
                // maintain target or use first match
                (None, single) | (single, None) => single,
            };
            // and remove done searches in the process
            s.entry().is_some()
        });
        let target = target?;

        trace!(?target, ?self.searches);

        // all searches have some matches, collect the lowest
        let matches = self
            .searches
            .iter_mut()
            .filter_map(|s| s.take_matches(target));
        let matches = MatchNode::group(Operator::Or, matches);
        Some(ExpressionSearchEvent::Found(target, matches))
    }

    #[instrument]
    fn poll_all(&mut self) -> Option<ExpressionSearchEvent> {
        let result = loop {
            if let Some(e) = self.ready.pop_front() {
                // let non-matching events be handled first, specifically load events
                break Some(e);
            }

            self.ready.extend(
                self.searches
                    .iter_mut()
                    // TODO: in parallel
                    .filter_map(|s| s.poll()),
            );

            if self.ready.is_empty() {
                break None;
            }
        };
        trace!(?result, ?self.searches);
        result
    }
}

impl Iterator for Union {
    type Item = ExpressionSearchEvent;

    fn next(&mut self) -> Option<Self::Item> {
        self.poll_all().or_else(|| self.complete_lowest())
    }
}

impl Debug for Search {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { search: _, matched } = self;
        f.debug_struct("Search")
            .field("matched", &matched)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use test_log::test;

    use super::*;
    #[test]
    fn next_result_or_alternation() {
        // Checking that OR results are correctly extracted in different scenarios
        for idx in 0..1 {
            for search0_pending in [false, true] {
                let entry_x = EntryIndex(1);
                let searches = vec![
                    Search {
                        matched: search0_pending.then_some((entry_x, MatchNode::empty())),
                        search: Box::new([].into_iter()),
                    },
                    Search {
                        matched: None,
                        search: Box::new(
                            [
                                ExpressionSearchEvent::Found(entry_x, MatchNode::empty()),
                                ExpressionSearchEvent::Stats(
                                    [(
                                        AttributeIndex(1),
                                        IndexSearchAttributeStats {
                                            frequencies: [(3.into(), 4)].into(),
                                            sizes: [(entry_x, (2, true))].into(),
                                        },
                                    )]
                                    .into_iter()
                                    .collect(),
                                ),
                            ]
                            .into_iter()
                            .inspect(|e| println!("idx1 producing {e:#?}")),
                        ),
                    },
                ];

                let mut sut = Union {
                    searches,
                    ready: [].into(),
                };

                match sut.next() {
                    Some(ExpressionSearchEvent::Found(e, _)) if e == entry_x => {}
                    result => panic!(
                        "expected found entry, got {result:?} for idx: {idx}, pending: {search0_pending}"
                    ),
                }

                match sut.next() {
                    Some(ExpressionSearchEvent::Stats(_)) => {}
                    result => panic!(
                        "expected stats, got {result:?} for idx: {idx}, pending: {search0_pending}"
                    ),
                }

                match sut.next() {
                    None => {}
                    result => panic!(
                        "expected None, got {result:?} for idx: {idx}, pending: {search0_pending}"
                    ),
                }
            }
        }
    }
}
