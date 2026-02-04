use super::*;
use crate::document::Value;
use crate::entry::Entry;

impl Document {
    /// Processes a `document` and its content to be ingestible in the index.
    ///
    /// returns the size of the doc and entry
    #[tracing::instrument(skip_all)]
    pub fn process(&self, processor: &dyn Processor) -> Entry {
        let Document {
            identifier,
            attributes,
        } = self;

        let attributes = attributes
            .iter()
            .fold(BTreeMap::<_, Vec<_>>::new(), |mut map, (name, value)| {
                map.entry(name).or_default().push(value);
                map
            })
            .into_iter()
            .map(|(name, values)| {
                let values = values
                    .into_iter()
                    .map(|value| match value {
                        Value::Text(text) => {
                            EntryValue::Text(processor.process_document_tokens(text.as_ref()))
                        }
                        Value::Tag(text) => EntryValue::Tag(text.clone()),
                        Value::Integer(int) => EntryValue::Integer(*int),
                        Value::Boolean(bool) => EntryValue::Boolean(*bool),
                    })
                    .map(Some)
                    .collect();
                (name.as_str().into(), Arc::new(values))
            })
            .collect();

        Entry::new(identifier.as_ref(), attributes)
    }
}
