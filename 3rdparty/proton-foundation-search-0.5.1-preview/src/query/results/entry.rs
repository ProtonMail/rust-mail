use std::hash::Hash;

use serde::{Deserialize, Serialize};

use crate::query::expression::Operator;
use crate::query::results::{MatchGroup, MatchValue, Score};

/// Result of an engine search,
/// either a simple match or a scored entry.
///
/// Matches can be returned early and progressively,
/// while scores need to be calculated and returned at the end.
///
/// An entry that has been matched before may therefore be scored later.
/// But one may also receive only a match or only a score.
#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
pub struct FoundEntry {
    identifier: Box<str>,
    matches: MatchGroup,
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl FoundEntry {
    /// Get the entry score if available
    pub fn score(&self) -> Score {
        self.matches.score()
    }

    /// Merge two found entries into one
    pub fn merge(&mut self, operator: Operator, other: Self) {
        debug_assert!(self.identifier == other.identifier);
        self.matches.merge(operator, other.matches);
    }
}

impl FoundEntry {
    /// Create a new found entry
    pub fn new(identifier: impl Into<Box<str>>) -> Self {
        Self::new_with_matches(identifier, MatchGroup::default())
    }

    /// Create a new found entry
    pub fn new_with_matches(identifier: impl Into<Box<str>>, matches: MatchGroup) -> Self {
        Self {
            identifier: identifier.into(),
            matches,
        }
    }

    /// Get the entry identifier
    pub fn identifier(&self) -> &str {
        self.identifier.as_ref()
    }

    /// Get the entry matched
    pub fn matches(&self) -> impl Iterator<Item = &MatchValue> {
        self.matches.matches()
    }

    /// Get the entry mutable matched
    pub fn matches_mut(&mut self) -> impl Iterator<Item = &mut MatchValue> {
        self.matches.matches_mut()
    }
}

impl Ord for FoundEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.score(), &self.identifier).cmp(&(other.score(), &other.identifier))
    }
}
impl PartialOrd for FoundEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[test]
fn scoring_unscored() {
    let sut = FoundEntry::new("unscored");
    assert_eq!(sut.score(), Score::NONE);
}

#[test]
fn scoring_and_or() {
    use crate::query::results::*;

    let sut = FoundEntry::new_with_matches(
        "scored",
        MatchGroup::new(
            Operator::And,
            [
                MatchNode::value(5, 0.5, []),
                MatchNode::group(
                    Operator::Or,
                    [MatchNode::value(2, 0.2, []), MatchNode::value(3, 0.3, [])],
                ),
            ],
        ),
    );
    assert_eq!(
        sut.score(),
        Score::new(0.3),
        "0.2 OR 0.3 => 0.3, 0.3 AND 0.5 => 0.3"
    );
}

#[test]
fn merging() {
    use crate::query::results::*;

    let mut sut = FoundEntry::new_with_matches(
        "scored",
        MatchGroup::new(
            Operator::And,
            [
                MatchNode::value(5, 0.5, []),
                MatchNode::group(
                    Operator::Or,
                    [MatchNode::value(2, 0.2, []), MatchNode::value(3, 0.3, [])],
                ),
            ],
        ),
    );

    sut.merge(
        Operator::And,
        FoundEntry::new_with_matches(
            "scored",
            MatchGroup::new(
                Operator::And,
                [
                    MatchNode::value(5, 0.5, []),
                    MatchNode::group(
                        Operator::Or,
                        [MatchNode::value(2, 0.2, []), MatchNode::value(3, 0.3, [])],
                    ),
                ],
            ),
        ),
    );

    sut.merge(
        Operator::Or,
        FoundEntry::new_with_matches(
            "scored",
            MatchGroup::new(
                Operator::And,
                [
                    MatchNode::value(5, 0.5, []),
                    MatchNode::group(
                        Operator::Or,
                        [MatchNode::value(2, 0.2, []), MatchNode::value(3, 0.3, [])],
                    ),
                ],
            ),
        ),
    );

    // Same operator groups are concatenated, different operators nested
    insta::assert_debug_snapshot!(sut, @r#"
    FoundEntry {
        identifier: "scored",
        matches: MatchGroup {
            operator: Or,
            nodes: [
                Group(
                    MatchGroup {
                        operator: And,
                        nodes: [
                            Value(
                                MatchValue {
                                    value: Integer(
                                        5,
                                    ),
                                    score: Score(
                                        0.5,
                                    ),
                                    occurrences: [],
                                },
                            ),
                            Group(
                                MatchGroup {
                                    operator: Or,
                                    nodes: [
                                        Value(
                                            MatchValue {
                                                value: Integer(
                                                    2,
                                                ),
                                                score: Score(
                                                    0.2,
                                                ),
                                                occurrences: [],
                                            },
                                        ),
                                        Value(
                                            MatchValue {
                                                value: Integer(
                                                    3,
                                                ),
                                                score: Score(
                                                    0.3,
                                                ),
                                                occurrences: [],
                                            },
                                        ),
                                    ],
                                },
                            ),
                            Value(
                                MatchValue {
                                    value: Integer(
                                        5,
                                    ),
                                    score: Score(
                                        0.5,
                                    ),
                                    occurrences: [],
                                },
                            ),
                            Group(
                                MatchGroup {
                                    operator: Or,
                                    nodes: [
                                        Value(
                                            MatchValue {
                                                value: Integer(
                                                    2,
                                                ),
                                                score: Score(
                                                    0.2,
                                                ),
                                                occurrences: [],
                                            },
                                        ),
                                        Value(
                                            MatchValue {
                                                value: Integer(
                                                    3,
                                                ),
                                                score: Score(
                                                    0.3,
                                                ),
                                                occurrences: [],
                                            },
                                        ),
                                    ],
                                },
                            ),
                        ],
                    },
                ),
                Group(
                    MatchGroup {
                        operator: And,
                        nodes: [
                            Value(
                                MatchValue {
                                    value: Integer(
                                        5,
                                    ),
                                    score: Score(
                                        0.5,
                                    ),
                                    occurrences: [],
                                },
                            ),
                            Group(
                                MatchGroup {
                                    operator: Or,
                                    nodes: [
                                        Value(
                                            MatchValue {
                                                value: Integer(
                                                    2,
                                                ),
                                                score: Score(
                                                    0.2,
                                                ),
                                                occurrences: [],
                                            },
                                        ),
                                        Value(
                                            MatchValue {
                                                value: Integer(
                                                    3,
                                                ),
                                                score: Score(
                                                    0.3,
                                                ),
                                                occurrences: [],
                                            },
                                        ),
                                    ],
                                },
                            ),
                        ],
                    },
                ),
            ],
        },
    }
    "#);
}
