//! Tree of matched values and their scores

use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::document::Value;
use crate::index::prelude::{
    AttributeIndex, EntryIndex, MatchedIndexTerm, TokenPosition, ValueIndex,
};
use crate::query::expression::Operator;
use crate::query::results::Score;

/// A group of matches
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MatchGroup<R: Realm = EngineRealm> {
    pub(crate) operator: Operator,
    pub(crate) nodes: Vec<MatchNode<R>>,
}

impl<R: Realm> MatchGroup<R> {
    /// Create a new match group
    pub fn new(
        operator: Operator,
        nodes: impl IntoIterator<Item = impl Into<MatchNode<R>>>,
    ) -> Self {
        Self {
            operator,
            nodes: nodes.into_iter().map(Into::into).collect(),
        }
    }

    /// Get all mutable matched values
    pub fn matches_mut(&mut self) -> impl Iterator<Item = &mut MatchValue<R>> {
        self.nodes.iter_mut().flat_map(|node| node.matches_mut())
    }

    /// Get all matched values
    pub fn matches(&self) -> impl Iterator<Item = &MatchValue<R>> {
        self.nodes.iter().flat_map(|node| node.matches())
    }

    /// Switch realms, resolving Realm specific attributes
    pub fn convert<R2: Realm>(
        self,
        attr: &dyn Fn(R::AttributeId) -> R2::AttributeId,
    ) -> MatchGroup<R2> {
        let Self { operator, nodes } = self;
        let nodes = nodes.into_iter().map(|m| m.convert(attr)).collect();
        MatchGroup { operator, nodes }
    }
}

impl<R: Realm> MatchGroup<R> {
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
                *self = Self::new(operator, [MatchNode::Group(this), MatchNode::Group(other)])
            }
        }
    }
}

impl<R: Realm> Default for MatchGroup<R> {
    fn default() -> Self {
        Self {
            operator: Operator::And,
            nodes: Default::default(),
        }
    }
}

/// A match item
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MatchNode<R: Realm = EngineRealm> {
    /// A matched value
    Value(MatchValue<R>),
    /// A nested matched group
    Group(MatchGroup<R>),
}

impl<R: Realm> MatchNode<R> {
    /// Create a new MatchNode::Value
    pub fn value(
        value: impl Into<Value>,
        score: impl Into<Score>,
        occurrences: impl IntoIterator<Item = impl Into<MatchOccurrence<R>>>,
    ) -> Self {
        Self::Value(MatchValue::new(
            value.into(),
            score.into(),
            occurrences.into_iter().map(Into::into).collect(),
        ))
    }

    /// Create a new MatchNode::Group
    pub fn group(
        operator: Operator,
        nodes: impl IntoIterator<Item = impl Into<MatchNode<R>>>,
    ) -> Self {
        let mut nodes = nodes
            .into_iter()
            .map(Into::into)
            .flat_map(|n| match n {
                MatchNode::Group(g) if g.operator == operator => g.nodes,
                _ => vec![n],
            })
            .collect::<Vec<_>>();

        if nodes.len() == 1 {
            nodes.remove(0)
        } else {
            Self::Group(MatchGroup { operator, nodes })
        }
    }

    /// Get the node's combined score
    pub fn score(&self) -> Score {
        match self {
            MatchNode::Value(value) => value.score,
            MatchNode::Group(group) => group.score(),
        }
    }

    /// Get all mutable matches recursively
    pub fn matches_mut(&mut self) -> impl Iterator<Item = &mut MatchValue<R>> {
        match self {
            MatchNode::Value(match_value) => vec![match_value],
            MatchNode::Group(match_group) => match_group.matches_mut().collect(),
        }
        .into_iter()
    }

    /// Get all matches recursively
    pub fn matches(&self) -> impl Iterator<Item = &MatchValue<R>> {
        match self {
            MatchNode::Value(match_value) => vec![match_value],
            MatchNode::Group(match_group) => match_group.matches().collect(),
        }
        .into_iter()
    }

    /// Switch realms, resolving Realm specific attributes
    pub fn convert<R2: Realm>(
        self,
        attr: &dyn Fn(R::AttributeId) -> R2::AttributeId,
    ) -> MatchNode<R2> {
        match self {
            MatchNode::Value(match_value) => MatchNode::Value(match_value.convert(attr)),
            MatchNode::Group(match_group) => MatchNode::Group(match_group.convert(attr)),
        }
    }

    /// Create an empty match node
    pub fn empty() -> Self {
        Self::Group(MatchGroup::default())
    }
}

impl<R: Realm> From<MatchNode<R>> for MatchGroup<R> {
    fn from(value: MatchNode<R>) -> Self {
        match value {
            v @ MatchNode::Value(_) => MatchGroup::new(Operator::And, [v]),
            MatchNode::Group(match_group) => match_group,
        }
    }
}

impl<R: Realm> From<MatchGroup<R>> for MatchNode<R> {
    fn from(value: MatchGroup<R>) -> Self {
        MatchNode::Group(value)
    }
}

impl From<MatchedIndexTerm> for MatchNode<IndexRealm> {
    fn from(m: MatchedIndexTerm) -> Self {
        Self::value(m.value, m.score, m.positions)
    }
}

/// A scored matched value
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MatchValue<R: Realm = EngineRealm> {
    pub(crate) value: Value,
    pub(crate) score: Score,
    pub(crate) occurrences: Vec<MatchOccurrence<R>>,
}

impl<R: Realm> MatchValue<R> {
    /// Create a new MatchValue
    pub fn new(value: Value, score: Score, occurrences: Vec<MatchOccurrence<R>>) -> Self {
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

    fn convert<R2: Realm>(
        self,
        attr: &dyn Fn(R::AttributeId) -> R2::AttributeId,
    ) -> MatchValue<R2> {
        let Self {
            value,
            score,
            occurrences,
        } = self;
        let occurrences = occurrences.into_iter().map(|o| o.convert(attr)).collect();
        MatchValue {
            value,
            score,
            occurrences,
        }
    }
}

impl<R: Realm> MatchValue<R> {
    /// Get matching score
    pub fn score(&self) -> Score {
        self.score
    }

    /// Get the matched occurrences
    pub fn occurrences(&self) -> Vec<MatchOccurrence<R>> {
        self.occurrences.clone()
    }
}

/// Location of a matched term
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MatchOccurrence<R: Realm = EngineRealm> {
    /// Name of the attribute
    attribute: R::AttributeId,
    /// value offset
    index: ValueIndex,
    /// position within the value
    position: TokenPosition,
}

impl<R: Realm> MatchOccurrence<R> {
    /// Create a new matched term
    pub fn new(
        attribute: impl Into<R::AttributeId>,
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
    pub fn attribute(&self) -> &<R::AttributeId as Deref>::Target {
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

    fn convert<R2: Realm>(
        self,
        attr: &dyn Fn(R::AttributeId) -> R2::AttributeId,
    ) -> MatchOccurrence<R2> {
        let Self {
            attribute,
            index,
            position,
        } = self;
        let attribute = attr(attribute);
        MatchOccurrence {
            attribute,
            index,
            position,
        }
    }
}

impl<A, I, P, R: Realm> From<(A, I, P)> for MatchOccurrence<R>
where
    A: Into<R::AttributeId>,
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

impl<A, I, P, R: Realm> From<MatchOccurrence<R>> for (A, I, P)
where
    R::AttributeId: Into<A>,
    ValueIndex: Into<I>,
    TokenPosition: Into<P>,
{
    fn from(
        MatchOccurrence {
            attribute,
            index,
            position,
        }: MatchOccurrence<R>,
    ) -> Self {
        (attribute.into(), index.into(), position.into())
    }
}

/// Facet abstraction for optional implementation
#[cfg(feature = "facet")]
pub trait Facet: for<'f> facet::Facet<'f> {}
#[cfg(feature = "facet")]
impl<T: for<'f> facet::Facet<'f>> Facet for T {}
/// Facet abstraction for optional implementation
#[cfg(not(feature = "facet"))]
pub trait Facet {}
#[cfg(not(feature = "facet"))]
impl<T> Facet for T {}

/// An index identifier
pub trait Id:
    core::fmt::Debug
    + Clone
    + Facet
    + Deref
    + PartialEq
    + Eq
    + PartialOrd
    + Ord
    + core::hash::Hash
    + Serialize
    + for<'de> Deserialize<'de>
{
}
impl<T> Id for T where
    T: core::fmt::Debug
        + Clone
        + Facet
        + Deref
        + PartialEq
        + Eq
        + PartialOrd
        + Ord
        + core::hash::Hash
        + Serialize
        + for<'de> Deserialize<'de>
{
}

/// A Realm selector unifies shared code among Engine and Index results
pub trait Realm: Clone {
    /// Attribute index identifier
    type AttributeId: Id;
    /// Entry index identifier
    type EntryId: Id;
}

/// The Index [`Realm`] specifies behavior/data specific to index search results
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct IndexRealm;
/// The Engine [`Realm`] specifies behavior/data specific to engine search results
#[cfg_attr(feature = "facet", derive(facet::Facet))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EngineRealm;

impl Realm for IndexRealm {
    type AttributeId = AttributeIndex;
    type EntryId = EntryIndex;
}
impl Realm for EngineRealm {
    type AttributeId = Box<str>;
    type EntryId = Box<str>;
}

#[test]
fn group_score() {
    let group = MatchGroup::new(
        Operator::Or,
        [
            // in OR, the highest score wins
            MatchNode::value(1, 0.1, None::<MatchOccurrence<EngineRealm>>),
            MatchNode::group(
                Operator::And,
                [
                    // in AND, the lowest one wins except for unscored match
                    MatchNode::value(0, Score::NONE, None::<MatchOccurrence<EngineRealm>>),
                    MatchNode::value(2, 0.2, None::<MatchOccurrence<EngineRealm>>),
                    MatchNode::value(3, 0.3, None::<MatchOccurrence<EngineRealm>>),
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
            MatchNode::value(1, 0.1, None::<MatchOccurrence<EngineRealm>>),
            MatchNode::group(
                Operator::And,
                [
                    // in AND, the lowest one wins except for unscored match
                    MatchNode::value(0, Score::NONE, None::<MatchOccurrence<EngineRealm>>),
                    MatchNode::value(2, 0.2, None::<MatchOccurrence<EngineRealm>>),
                    MatchNode::value(3, 0.3, None::<MatchOccurrence<EngineRealm>>),
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
