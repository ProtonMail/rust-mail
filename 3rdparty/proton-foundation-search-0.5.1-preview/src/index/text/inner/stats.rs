use super::*;

/// Statistics for the text index.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stats {
    /// Number of tokens per entry attribute.
    sizes: BTreeMap<AttributeIndex, BTreeMap<EntryIndex, (usize, usize)>>,
}

impl Stats {
    /// Increase stats
    ///
    /// length: token length in bytes
    /// count: number of token occurrences
    pub fn add(
        &mut self,
        entry: EntryIndex,
        attribute: AttributeIndex,
        length: usize,
        count: usize,
    ) {
        let total = length * count;
        if total == 0 {
            return;
        }
        let (attr_length, attr_count) = self
            .sizes
            .entry(attribute)
            .or_default()
            .entry(entry)
            .or_default();
        *attr_length += total;
        *attr_count += count
    }

    /// Decrease stats
    ///
    /// len: token length in bytes
    /// count: number of token occurrences
    pub fn sub(&mut self, entry: EntryIndex, attribute: AttributeIndex, len: usize, count: usize) {
        let total = len * count;
        if total == 0 {
            return;
        }
        let (attr_length, attr_count) = self
            .sizes
            .entry(attribute)
            .or_default()
            .entry(entry)
            .or_default();

        assert!(
            *attr_length >= total,
            "cannot have negative length: {attr_length} - {total}"
        );
        assert!(
            *attr_count >= count,
            "cannot have negative count: {attr_count} - {count}"
        );

        *attr_length -= total;
        *attr_count -= count;

        assert!(
            (*attr_length == 0 && *attr_count == 0) || (*attr_length != 0 && *attr_count != 0),
            "either length and count are both zero, or both non-zero: length: {attr_length}, count: {attr_count}"
        );

        if *attr_length == 0 || *attr_count == 0 {
            // remove empty entries
            self.remove(entry, attribute);
        }
    }

    /// Remove stats
    ///
    /// returns (length, count)
    /// length: token length in bytes
    /// count: number of token occurrences
    pub fn remove(&mut self, entry: EntryIndex, attribute: AttributeIndex) -> (usize, usize) {
        if let Some(attr) = self.sizes.get_mut(&attribute) {
            let removed = attr.remove(&entry);
            if removed.is_some() && attr.is_empty() {
                self.sizes.remove(&attribute);
            }
            return removed.unwrap_or_default();
        }
        Default::default()
    }

    /// Set stats
    ///
    /// len: token length in bytes
    /// count: number of token occurrences
    pub fn set(
        &mut self,
        entry: EntryIndex,
        attribute: AttributeIndex,
        length: usize,
        count: usize,
    ) {
        self.sizes
            .entry(attribute)
            .or_default()
            .insert(entry, (length, count));
    }

    /// Get total token count in an attribute
    pub fn entries(&self, attribute: AttributeIndex) -> usize {
        self.sizes
            .get(&attribute)
            .map(|entries: &BTreeMap<EntryIndex, (usize, usize)>| entries.len())
            .unwrap_or_default()
    }

    /// Get total token count in an attribute
    pub fn count(&self, attribute: AttributeIndex) -> usize {
        self.sizes
            .get(&attribute)
            .map(|attr| attr.values())
            .into_iter()
            .flatten()
            .map(|(_length, count)| count)
            .sum()
    }

    /// Get total byte-wise size
    pub fn length(&self) -> usize {
        self.sizes
            .values()
            .flat_map(|a| a.values())
            .map(|(length, _count)| length)
            .sum()
    }

    /// Get (length,count) for given entry attribute
    pub(crate) fn get(
        &self,
        entry: EntryIndex,
        attribute: AttributeIndex,
    ) -> Option<(usize, usize)> {
        self.sizes
            .get(&attribute)
            .and_then(|attr| attr.get(&entry))
            .copied()
    }

    /// Get stats per attribute
    pub fn attribute(
        &self,
        attribute: AttributeIndex,
    ) -> (usize, f64, BTreeMap<EntryIndex, usize>) {
        let Some(attr) = self.sizes.get(&attribute) else {
            return Default::default();
        };

        let entries = attr.len();
        let size = attr
            .values()
            .map(|(_length, count)| *count as f64)
            .sum::<f64>()
            / entries as f64;
        let sizes = attr
            .iter()
            .map(|(e, (_length, count))| (*e, *count))
            .collect();
        (entries, size, sizes)
    }
}
