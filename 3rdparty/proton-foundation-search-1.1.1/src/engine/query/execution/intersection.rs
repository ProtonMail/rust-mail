use std::collections::VecDeque;

use tracing::trace;

use super::*;
use crate::index::prelude::*;
use crate::query::results::*;

pub struct Intersection {
    /// Searches to intersect, assuming that results are sorted by EntryIndex
    searches: Vec<Box<dyn Send + Iterator<Item = ExpressionSearchEvent>>>,
    /// Next prospective entry
    target: EntryIndex,
    /// Collecting matches for an entry
    matches: Vec<Option<MatchNode<IndexRealm>>>,
    /// Flag that all possible results have been found
    done: bool,
    /// Flag that some results have been found
    some: bool,
    /// Outbound events
    ready: VecDeque<ExpressionSearchEvent>,
}

impl Intersection {
    pub fn new(searches: Vec<Box<dyn Send + Iterator<Item = ExpressionSearchEvent>>>) -> Self {
        Self {
            target: EntryIndex(0),
            matches: vec![None; searches.len()],
            searches,
            done: false,
            some: false,
            ready: [].into(),
        }
    }

    #[instrument]
    fn poll_search(&mut self, i: usize) {
        let Self {
            searches,
            target,
            matches,
            done,
            some,
            ready,
        } = self;
        let len = searches.len();
        let result = loop {
            let Some(event) = searches[i].next() else {
                *done = true;
                break None;
            };
            break match event {
                ExpressionSearchEvent::Found(entry_index, matched) => {
                    trace!(found = ?entry_index, ?target, ?matches);

                    if entry_index < *target {
                        continue;
                    } else if entry_index > *target {
                        *target = entry_index;
                        *matches = vec![None; len];
                        matches[i] = Some(matched);
                    } else {
                        // entry_index == target
                        matches[i] = Some(matched);
                    }
                    if matches.iter().all(|m| m.is_some()) {
                        let e = core::mem::replace(target, EntryIndex(0));
                        let m = core::mem::replace(matches, vec![None; len]);
                        *some = true;
                        Some(complete_pending(e, m))
                    } else {
                        // swallow event
                        None
                    }
                }
                other => Some(other),
            };
        };

        trace!(?result, ?target, ?done, ?some, ?matches);
        ready.extend(result);
    }

    fn poll(&mut self) {
        self.matches
            .iter()
            .enumerate()
            .filter_map(|(i, m)| m.is_none().then_some(i))
            .collect::<Vec<_>>()
            .into_iter()
            // TODO: parallel
            .for_each(|i| self.poll_search(i))
    }

    /// Assuming some and all posible results have been returned
    /// poll each search for stats
    #[instrument(skip(self))]
    fn collect_remaining_stats(&mut self) -> Option<ExpressionSearchEvent> {
        if !self.some {
            return None;
        }
        for search in self.searches.iter_mut() {
            for event in search {
                match event {
                    ExpressionSearchEvent::Found(..) => {
                        //ignore
                    }
                    other => {
                        return Some(other);
                    }
                }
            }
        }
        None
    }
}

impl Iterator for Intersection {
    type Item = ExpressionSearchEvent;

    #[instrument]
    fn next(&mut self) -> Option<Self::Item> {
        let result = loop {
            if let Some(event) = self.ready.pop_front() {
                break Some(event);
            }

            if self.done {
                break self.collect_remaining_stats();
            } else {
                while !self.done && self.ready.is_empty() {
                    self.poll();
                }
                continue;
            }
        };
        trace!(?result);
        result
    }
}

fn complete_pending(
    target: EntryIndex,
    matches: Vec<Option<MatchNode<IndexRealm>>>,
) -> ExpressionSearchEvent {
    ExpressionSearchEvent::Found(
        target,
        MatchNode::group(Operator::And, matches.into_iter().flatten()),
    )
}

impl Debug for Intersection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            searches,
            target,
            matches,
            done,
            some,
            ready,
        } = self;
        f.debug_struct("Intersection")
            .field("searches", &searches.len())
            .field("target", &target)
            .field("matches", &matches)
            .field("done", &done)
            .field("some", &some)
            .field("ready", &ready)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use test_log::test;
    use tracing::trace;

    use super::*;

    #[test]
    fn next_result_and_alternation() {
        // Checking that AND results are correctly extracted in different scenarios
        for search0_pending in [false, true] {
            let entry_1 = EntryIndex(1);
            let entry_2 = EntryIndex(2);
            let entry_3 = EntryIndex(3);
            let searches = vec![
                Box::new(
                    [ExpressionSearchEvent::Found(entry_2, MatchNode::empty())]
                        .into_iter()
                        .inspect(|e| trace!("idx0 produced {e:#?}")),
                ) as Box<dyn Send + Iterator<Item = ExpressionSearchEvent>>,
                Box::new(
                    [
                        ExpressionSearchEvent::Found(entry_1, MatchNode::empty()),
                        ExpressionSearchEvent::Found(entry_3, MatchNode::empty()),
                        ExpressionSearchEvent::Stats(
                            [(
                                AttributeIndex(1),
                                IndexSearchAttributeStats {
                                    frequencies: [(3.into(), 4)].into(),
                                    sizes: [(entry_1, (2, true))].into(),
                                },
                            )]
                            .into_iter()
                            .collect(),
                        ),
                    ]
                    .into_iter()
                    .inspect(|e| trace!("idx1 produced {e:#?}")),
                ),
            ];
            let pending = [search0_pending, false]
                .into_iter()
                .map(|pending| pending.then_some(MatchNode::empty()))
                .collect();

            let mut sut = Intersection {
                searches,
                target: entry_1,
                matches: pending,
                done: false,
                some: false,
                ready: [].into(),
            };

            if search0_pending {
                // both searches produce the same entry
                match sut.next() {
                    Some(ExpressionSearchEvent::Found(e, _)) if e == entry_1 => {}
                    result => {
                        panic!("expected found entry 1, got {result:?}, pending: {search0_pending}")
                    }
                }

                match sut.next() {
                    Some(ExpressionSearchEvent::Stats(_)) => {}
                    result => panic!(
                        "expected stats, got {result:?}, pending: {search0_pending}, sut: {sut:#?}"
                    ),
                }

                match sut.next() {
                    None => {}
                    result => panic!("expected None, got {result:?}, pending: {search0_pending}"),
                }
            } else {
                // only one search produces the entry => no match, no stats
                match sut.next() {
                    None => {}
                    result => panic!("expected None, got {result:?}, pending: {search0_pending}"),
                }
            }
        }
    }

    #[test]
    fn next_result_and_converging() {
        // Checking that AND results are correctly extracted in different scenarios
        let entry_1 = EntryIndex(1);
        let entry_2 = EntryIndex(2);
        let entry_3 = EntryIndex(3);
        let searches = vec![
            Box::new(
                [
                    ExpressionSearchEvent::Found(entry_2, MatchNode::empty()),
                    ExpressionSearchEvent::Found(entry_3, MatchNode::empty()),
                ]
                .into_iter()
                .inspect(|e| trace!("idx0 produced {e:#?}")),
            ) as Box<dyn Send + Iterator<Item = ExpressionSearchEvent>>,
            Box::new(
                [
                    ExpressionSearchEvent::Found(entry_1, MatchNode::empty()),
                    ExpressionSearchEvent::Found(entry_3, MatchNode::empty()),
                    ExpressionSearchEvent::Stats(
                        [(
                            AttributeIndex(1),
                            IndexSearchAttributeStats {
                                frequencies: [(3.into(), 4)].into(),
                                sizes: [(entry_1, (2, true))].into(),
                            },
                        )]
                        .into_iter()
                        .collect(),
                    ),
                ]
                .into_iter()
                .inspect(|e| trace!("idx1 produced {e:#?}")),
            ),
        ];

        let mut sut = Intersection::new(searches);

        // only entry 3 matches both

        match sut.next() {
            Some(ExpressionSearchEvent::Found(e, _)) if e == entry_3 => {}
            result => {
                panic!("expected found entry 3, got {result:?}, sut: {sut:#?}")
            }
        }

        match sut.next() {
            Some(ExpressionSearchEvent::Stats(_)) => {}
            result => {
                panic!("expected stats, got {result:?}, sut: {sut:#?}")
            }
        }

        match sut.next() {
            None => {}
            result => panic!("expected None, got {result:?}, sut: {sut:#?}"),
        }
    }
}
