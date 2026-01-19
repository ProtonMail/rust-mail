use super::*;

impl TextIndex {
    pub(crate) fn export(
        &self,
    ) -> impl 'static + Send + Iterator<Item = (EntryIndex, AttributeIndex, EntryValues)> {
        let entries = self.tokens.iter().fold(
            BTreeMap::new(),
            |mut map: BTreeMap<(EntryIndex, AttributeIndex), BTreeSet<_>>, (token, v)| {
                for (occurrence_ref, positions) in v {
                    let occurrence = self.occurrence(*occurrence_ref);
                    let entry = map.entry(occurrence).or_default();
                    for (index, placements) in positions {
                        for position in placements {
                            entry.insert((*index, *position, token.clone()));
                        }
                    }
                }
                map
            },
        );
        entries.into_iter().map(|((entry, attribute), tokens)| {
            let mut text =
                vec![EntryValue::Empty; tokens.last().map(|(v, ..)| v.0).unwrap_or_default() + 1];
            tokens
                .into_iter()
                .chunk_by(|(v, ..)| *v)
                .into_iter()
                .map(|(index, tokens)| {
                    (index, tokens.map(|(_v, p, t)| (p.0, t)).collect::<Vec<_>>())
                })
                .for_each(|(index, value)| {
                    text[index.0] = EntryValue::Text(value);
                });
            (entry, attribute, text)
        })
    }
}

#[cfg(test)]
mod tests;
