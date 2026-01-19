//! Tree of matched values and their scores

use serde::{Deserialize, Serialize};

use crate::document::Value;
use crate::index::prelude::{TokenPosition, ValueIndex};
use crate::query::expression::Operator;
use crate::query::results::Score;

/// A group of matches
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MatchGroup {
    pub(crate) operator: Operator,
    pub(crate) nodes: Vec<MatchNode>,
}

impl MatchGroup {
    /// Create a new match group
    pub fn new(operator: Operator, nodes: impl IntoIterator<Item = MatchNode>) -> MatchGroup {
        Self {
            operator,
            nodes: nodes.into_iter().collect(),
        }
    }

    /// Get all mutable matched values
    pub fn matches_mut(&mut self) -> impl Iterator<Item = &mut MatchValue> {
        self.nodes.iter_mut().flat_map(|node| node.matches_mut())
    }

    /// Get all matched values
    pub fn matches(&self) -> impl Iterator<Item = &MatchValue> {
        self.nodes.iter().flat_map(|node| node.matches())
    }
}

impl MatchGroup {
    /// Get the group's combined score
    pub fn score(&self) -> Score {
        self.nodes.iter().fold(Score::NONE, |mut score, node| {
            score.merge(self.operator, node.score());
            score
        })
    }

    /// Merge two groups together using an explicit operator
    pub fn merge(&mut self, operator: Operator, mut other: Self) {
        match (
            self.operator,
            operator,
            other.operator,
            self.nodes.as_mut_slice(),
            other.nodes.as_mut_slice(),
        ) {
            (Operator::And, Operator::And, Operator::And, _, _)
            | (Operator::Or, Operator::Or, Operator::Or, _, _) => {
                self.nodes.extend(other.nodes);
            }
            // optimize case where there is just one node in the other group
            (Operator::And, Operator::And, _, _, [_only_one])
            | (Operator::Or, Operator::Or, _, _, [_only_one]) => {
                #[allow(clippy::expect_used, reason = "checked")]
                let single = other.nodes.pop().expect("checked there is one");
                self.nodes.push(single);
            }
            // optimize case where there is just one node in the self group
            (_, Operator::And, Operator::And, [_only_one], _)
            | (_, Operator::Or, Operator::Or, [_only_one], _) => {
                #[allow(clippy::expect_used, reason = "checked")]
                let single = self.nodes.pop().expect("checked there is one");
                other.nodes.insert(0, single);
                *self = other;
            }
            // optimize case where there is just one node on both sides
            (_, _, _, [_only_one], [_just_one]) => {
                #[allow(clippy::expect_used, reason = "checked")]
                let single = other.nodes.pop().expect("checked there is one");
                self.nodes.push(single);
                self.operator = operator;
            }
            // optimize case when self is empty
            (_, _, _, [], _) => *self = other,
            // optimize case when other is empty
            (_, _, _, _, []) => { /*noop*/ }
            // otherwise nest the tree
            _ => {
                let this = std::mem::take(self);
                *self = MatchGroup::new(operator, [MatchNode::Group(this), MatchNode::Group(other)])
            }
        }
    }
}

impl Default for MatchGroup {
    fn default() -> Self {
        Self {
            operator: Operator::And,
            nodes: Default::default(),
        }
    }
}

/// A match item
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum MatchNode {
    /// A matched value
    Value(MatchValue),
    /// A nested matched group
    Group(MatchGroup),
}

impl MatchNode {
    /// Create a new MatchNode::Value
    pub fn value(
        value: impl Into<Value>,
        score: impl Into<Score>,
        occurrences: impl IntoIterator<Item = MatchOccurrence>,
    ) -> MatchNode {
        Self::Value(MatchValue::new(
            value.into(),
            score.into(),
            occurrences.into_iter().collect(),
        ))
    }
    /// Create a new MatchNode::Group
    pub fn group(operator: Operator, nodes: impl IntoIterator<Item = MatchNode>) -> Self {
        Self::Group(MatchGroup::new(operator, nodes))
    }

    /// Get the node's combined score
    pub fn score(&self) -> Score {
        match self {
            MatchNode::Value(value) => value.score,
            MatchNode::Group(group) => group.score(),
        }
    }

    /// Get all mutable matches recursively
    pub fn matches_mut(&mut self) -> impl Iterator<Item = &mut MatchValue> {
        match self {
            MatchNode::Value(match_value) => vec![match_value],
            MatchNode::Group(match_group) => match_group.matches_mut().collect(),
        }
        .into_iter()
    }

    /// Get all matches recursively
    pub fn matches(&self) -> impl Iterator<Item = &MatchValue> {
        match self {
            MatchNode::Value(match_value) => vec![match_value],
            MatchNode::Group(match_group) => match_group.matches().collect(),
        }
        .into_iter()
    }
}

/// A scored matched value
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MatchValue {
    pub(crate) value: Value,
    pub(crate) score: Score,
    pub(crate) occurrences: Vec<MatchOccurrence>,
}

impl MatchValue {
    /// Create a new MatchValue
    pub fn new(value: Value, score: Score, occurrences: Vec<MatchOccurrence>) -> Self {
        Self {
            value,
            score,
            occurrences,
        }
    }

    /// Get the matched value
    pub fn value(&self) -> &Value {
        &self.value
    }
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl MatchValue {
    /// Get matching score
    pub fn score(&self) -> Score {
        self.score
    }

    /// Get the matched occurrences
    pub fn occurrences(&self) -> Vec<MatchOccurrence> {
        self.occurrences.clone()
    }
}

/// Location of a matched term
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MatchOccurrence {
    /// Name of the attribute
    attribute: Box<str>,
    /// value offset
    index: ValueIndex,
    /// position within the value
    position: TokenPosition,
}

impl MatchOccurrence {
    /// Create a new matched term
    pub fn new(
        attribute: impl Into<Box<str>>,
        index: impl Into<ValueIndex>,
        position: impl Into<TokenPosition>,
    ) -> Self {
        Self {
            attribute: attribute.into(),
            index: index.into(),
            position: position.into(),
        }
    }

    /// Get matched attribute
    pub fn attribute(&self) -> &str {
        &self.attribute
    }

    /// Get matched value index (offset within the attribute value set)
    pub fn index(&self) -> ValueIndex {
        self.index
    }

    /// Get matched token position (positional value associated with a text token)
    pub fn position(&self) -> TokenPosition {
        self.position
    }
}

impl<A, I, P> From<(A, I, P)> for MatchOccurrence
where
    A: Into<Box<str>>,
    I: Into<ValueIndex>,
    P: Into<TokenPosition>,
{
    fn from((a, i, p): (A, I, P)) -> Self {
        MatchOccurrence {
            attribute: a.into(),
            index: i.into(),
            position: p.into(),
        }
    }
}

#[test]
fn group_score() {
    let group = MatchGroup::new(
        Operator::Or,
        [
            // in OR, the highest score wins
            MatchNode::value(1, 0.1, []),
            MatchNode::group(
                Operator::And,
                [
                    // in AND, the lowest one wins except for unscored match
                    MatchNode::value(0, Score::NONE, []),
                    MatchNode::value(2, 0.2, []),
                    MatchNode::value(3, 0.3, []),
                ],
            ),
        ],
    );

    let score = group.score();
    assert_eq!(*score, 0.2,)
}

#[test]
fn recursive_matches() {
    let mut group = MatchGroup::new(
        Operator::Or,
        [
            // in OR, the highest score wins
            MatchNode::value(1, 0.1, []),
            MatchNode::group(
                Operator::And,
                [
                    // in AND, the lowest one wins except for unscored match
                    MatchNode::value(0, Score::NONE, []),
                    MatchNode::value(2, 0.2, []),
                    MatchNode::value(3, 0.3, []),
                ],
            ),
        ],
    );

    let matches = group.matches().collect::<Vec<_>>();
    insta::assert_debug_snapshot!(matches, @r"
    [
        MatchValue {
            value: Integer(
                1,
            ),
            score: Score(
                0.1,
            ),
            occurrences: [],
        },
        MatchValue {
            value: Integer(
                0,
            ),
            score: Score(
                0.0,
            ),
            occurrences: [],
        },
        MatchValue {
            value: Integer(
                2,
            ),
            score: Score(
                0.2,
            ),
            occurrences: [],
        },
        MatchValue {
            value: Integer(
                3,
            ),
            score: Score(
                0.3,
            ),
            occurrences: [],
        },
    ]
    ");

    let matches = group.matches_mut().collect::<Vec<_>>();
    insta::assert_debug_snapshot!(matches, @r"
    [
        MatchValue {
            value: Integer(
                1,
            ),
            score: Score(
                0.1,
            ),
            occurrences: [],
        },
        MatchValue {
            value: Integer(
                0,
            ),
            score: Score(
                0.0,
            ),
            occurrences: [],
        },
        MatchValue {
            value: Integer(
                2,
            ),
            score: Score(
                0.2,
            ),
            occurrences: [],
        },
        MatchValue {
            value: Integer(
                3,
            ),
            score: Score(
                0.3,
            ),
            occurrences: [],
        },
    ]
    ");
}
