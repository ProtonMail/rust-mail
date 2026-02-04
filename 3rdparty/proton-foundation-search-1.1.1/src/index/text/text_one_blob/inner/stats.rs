use super::*;

/// Statistics for the text index.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Hash)]
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

    pub fn update_stats(&self, stats: &mut IndexSearchStats) {
        for (a, entries) in &self.sizes {
            for (e, (_length, count)) in entries {
                stats.increment_size(*a, *e, *count);
            }
        }
    }

    /// Get total byte-wise size
    #[cfg(test)]
    pub fn length(&self) -> usize {
        self.sizes
            .values()
            .flat_map(|a| a.values())
            .map(|(length, _count)| length)
            .sum()
    }
}
