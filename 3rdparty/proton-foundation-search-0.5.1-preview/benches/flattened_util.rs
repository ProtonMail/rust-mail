use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::hash::{Hash, Hasher as _};
use std::io::{BufReader, BufWriter};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Simple token reference for the flattened index
/// Uses hash of token string for deterministic, segment-independent allocation
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct TokenRef(pub u64);

impl From<&str> for TokenRef {
    fn from(value: &str) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        value.hash(&mut hasher);
        TokenRef(hasher.finish())
    }
}

/// Simple position types for the flattened index
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct TokenPosition(pub u32);

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct TrigramPosition(pub u8);

impl TrigramPosition {
    pub fn new(pos: u8) -> Self {
        TrigramPosition(pos)
    }
}

/// Simple index types
pub type EntryIndex = u32;
pub type AttributeIndex = u32;
pub type ValueIndex = u32;

/// Composite key for flattened token occurrences
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct CompositeKey {
    pub token_ref: TokenRef,
    pub entry_index: EntryIndex,
    pub attribute_index: AttributeIndex,
    pub value_index: ValueIndex,
    pub token_position: TokenPosition,
}

/// Composite key for flattened trigram mappings
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct TrigramKey {
    pub trigram: Arc<str>,
    pub position: TrigramPosition,
    pub token_ref: TokenRef,
}

/// Context for token insertion
#[derive(Debug, Clone)]
pub struct TokenContext {
    pub entry_index: EntryIndex,
    pub attribute_index: AttributeIndex,
    pub value_index: ValueIndex,
    pub token_position: TokenPosition,
}

/// Simple trigram generation trait
pub trait Trigrams {
    fn trigrams(&self) -> impl Iterator<Item = (usize, String)>;
}

impl Trigrams for str {
    fn trigrams(&self) -> impl Iterator<Item = (usize, String)> {
        let chars: Vec<char> = self.chars().collect();
        (0..chars.len().saturating_sub(2)).map(move |i| {
            let trigram: String = chars[i..i + 3].iter().collect();
            (i, trigram)
        })
    }
}

/// Flattened text index for fast O(1) operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlattenedTextIndex {
    pub token_occurrences: HashSet<CompositeKey>,
    pub trigram_mappings: HashSet<TrigramKey>,
    /// Maps token strings to their TokenRef for hash-based allocation
    /// Note: With hash-based TokenRefs, this is mainly for reverse lookup
    pub token_to_ref: HashMap<Box<str>, TokenRef>,
}

impl CompositeKey {
    pub fn from_context(context: TokenContext, token_ref: TokenRef) -> Self {
        Self {
            token_ref,
            entry_index: context.entry_index,
            attribute_index: context.attribute_index,
            value_index: context.value_index,
            token_position: context.token_position,
        }
    }
}

impl TrigramKey {
    pub fn new(trigram: Arc<str>, position: TrigramPosition, token_ref: TokenRef) -> Self {
        Self {
            trigram,
            position,
            token_ref,
        }
    }
}

impl FlattenedTextIndex {
    /// Fast O(1) insert for a token
    pub fn insert_token(&mut self, token: &str, context: TokenContext) -> bool {
        // Get or create TokenRef for this token (hash-based allocation)
        let token_ref = if let Some(existing_ref) = self.token_to_ref.get(token) {
            *existing_ref
        } else {
            let new_ref = TokenRef::from(token);
            self.token_to_ref.insert(token.into(), new_ref);
            new_ref
        };

        // Insert token occurrence
        let key = CompositeKey::from_context(context.clone(), token_ref);
        let was_new = self.token_occurrences.insert(key.clone());

        // Insert trigrams
        for (pos, trigram) in token.trigrams() {
            let trigram_key = TrigramKey::new(
                Arc::from(trigram),
                TrigramPosition::new(pos as u8),
                token_ref,
            );
            self.trigram_mappings.insert(trigram_key);
        }

        was_new
    }

    /// Get statistics about the index
    #[allow(dead_code)]
    pub fn stats(&self) -> (usize, usize, usize) {
        (
            self.token_to_ref.len(),
            self.token_occurrences.len(),
            self.trigram_mappings.len(),
        )
    }

    /// Serialize the index to a CBOR file, compressed with zstd
    pub fn serialize_to_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        let mut encoder = zstd::Encoder::new(writer, 0)?;
        ciborium::ser::into_writer(self, &mut encoder)?;
        encoder.finish()?;
        Ok(())
    }

    /// Deserialize the index from a zstd-compressed CBOR file
    pub fn deserialize_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut decoder = zstd::Decoder::new(reader)?;
        let index: FlattenedTextIndex = ciborium::de::from_reader(&mut decoder)?;
        Ok(index)
    }

    /// Optimized hierarchical conversion that leverages fast flattened structure iteration
    /// This version avoids expensive grouping and sorting operations by using direct iteration
    pub fn to_hierarchical(&self) -> HierarchicalTextIndex {
        // Pre-allocate with known sizes for better performance

        // Create reverse mapping from TokenRef to token string for O(1) lookup
        let ref_to_token: HashMap<TokenRef, Box<str>> = self
            .token_to_ref
            .iter()
            .map(|(token, token_ref)| (*token_ref, token.clone()))
            .collect();

        // Build occurrence index using iterators
        let unique_occurrences: Vec<_> = self
            .token_occurrences
            .iter()
            .map(|key| (key.entry_index, key.attribute_index))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        // Create occurrence mapping
        let occurrence_map: HashMap<(EntryIndex, AttributeIndex), u32> = unique_occurrences
            .into_iter()
            .enumerate()
            .map(|(idx, occurrence_key)| (occurrence_key, idx as u32))
            .collect();

        // Build token occurrences using iterators and fold
        let token_occurrences_map: HashMap<
            TokenRef,
            HashMap<u32, HashMap<ValueIndex, BTreeSet<TokenPosition>>>,
        > = self
            .token_occurrences
            .iter()
            .fold(HashMap::new(), |mut acc, key| {
                let occurrence_ref = occurrence_map[&(key.entry_index, key.attribute_index)];
                acc.entry(key.token_ref)
                    .or_default()
                    .entry(occurrence_ref)
                    .or_default()
                    .entry(key.value_index)
                    .or_default()
                    .insert(key.token_position);
                acc
            });

        // Build final token structure using iterators - collect first, then sort
        let mut token_entries: Vec<_> = token_occurrences_map
            .into_iter()
            .map(|(token_ref, token_occurrences)| {
                let token_string = ref_to_token
                    .get(&token_ref)
                    .cloned()
                    .unwrap_or_else(|| format!("{:?}", token_ref).into_boxed_str());
                (token_ref, token_string, token_occurrences)
            })
            .collect();

        // Sort by token_ref
        token_entries.sort_by_key(|(token_ref, _, _)| *token_ref);

        // Extract the final tokens
        let tokens = token_entries
            .into_iter()
            .map(|(_, token_string, token_occurrences)| (token_string, token_occurrences))
            .collect();

        // Build trigrams using iterators
        let trigrams: BTreeMap<Arc<str>, BTreeMap<TrigramPosition, BTreeSet<TokenRef>>> = self
            .trigram_mappings
            .iter()
            .fold(BTreeMap::new(), |mut acc, trigram_key| {
                acc.entry(trigram_key.trigram.clone())
                    .or_default()
                    .entry(trigram_key.position)
                    .or_default()
                    .insert(trigram_key.token_ref);
                acc
            });

        HierarchicalTextIndex { tokens, trigrams }
    }

    /// Compact multiple segments into a single index
    /// This additively combines segments as though all data was loaded in one batch
    pub fn compact(segments: Vec<FlattenedTextIndex>) -> FlattenedTextIndex {
        let mut result = FlattenedTextIndex::default();

        for segment in segments {
            // Merge token_occurrences (additive)
            result.token_occurrences.extend(segment.token_occurrences);

            // Merge trigram_mappings (additive)
            result.trigram_mappings.extend(segment.trigram_mappings);

            // Merge token_to_ref mappings
            // With hash-based TokenRefs, same token will have same TokenRef across segments
            result.token_to_ref.extend(segment.token_to_ref);
        }

        result
    }

    /// Compact the index by removing unused tokens and optimizing storage
    /// Note: With hash-based TokenRefs, this is less necessary but still useful for cleanup
    #[allow(dead_code)]
    pub fn compact_remove_unused(&mut self) {
        // Find tokens that are actually used
        let used_tokens: HashSet<TokenRef> = self
            .token_occurrences
            .iter()
            .map(|key| key.token_ref)
            .collect();

        // Remove unused tokens from token_to_ref
        self.token_to_ref
            .retain(|_, token_ref| used_tokens.contains(token_ref));
    }
}

/// Type alias for token occurrences to reduce complexity
type TokenOccurrences = HashMap<u32, HashMap<ValueIndex, BTreeSet<TokenPosition>>>;

/// Hierarchical text index structure (similar to the existing TextIndex)
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct HierarchicalTextIndex {
    pub tokens: Vec<(Box<str>, TokenOccurrences)>,
    pub trigrams: BTreeMap<Arc<str>, BTreeMap<TrigramPosition, BTreeSet<TokenRef>>>,
}

impl HierarchicalTextIndex {
    #[allow(dead_code)]
    pub fn stats(&self) -> (usize, usize) {
        let total_occurrences = self
            .tokens
            .iter()
            .flat_map(|(_, token_occurrences)| token_occurrences.values())
            .flat_map(|value_occurrences| value_occurrences.values())
            .map(|positions| positions.len())
            .sum();
        (self.tokens.len(), total_occurrences)
    }
}
