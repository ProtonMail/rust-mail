//! Explicitly WASM specific implementations and types

use std::sync::Arc;

use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::blob::{BlobId, Cached, LoadEvent};
use crate::document::Document;
use crate::engine::{Cleanup, Engine, Execution, InnerEngine, Query, Search, Stats, Write};
use crate::processor::Processor;
use crate::query::expression::wasm::Expression;
use crate::query::results::FoundEntry;
use crate::query::stats::CollectionStats;
use crate::serialization::SerDes;

#[wasm_bindgen]
impl Engine {
    /// Get the configured processor for this engine
    #[wasm_bindgen(js_name = "processor")]
    pub fn processor_wasm(&self) -> EngineProcessor {
        EngineProcessor {
            inner: self.inner.clone(),
        }
    }
}

/// A Processor configured for an engine for the WASM API
#[derive(Debug, Clone)]
#[wasm_bindgen]
pub struct EngineProcessor {
    inner: Arc<InnerEngine>,
}

#[wasm_bindgen]
impl EngineProcessor {
    /// Convert Document Values to Entry values
    #[wasm_bindgen(js_name = "processDocumentTokens")]
    pub fn wasm_tokens(&self, value: &str) -> Vec<TextToken> {
        self.process_document_tokens(value)
            .into_iter()
            .map(|(position, text)| TextToken {
                position,
                text: text.into(),
            })
            .collect()
    }

    /// Tokenize query text so it can be matched against IndexedValue text
    #[wasm_bindgen(js_name = "processQueryTerm")]
    pub fn wasm_term(&self, term: &str) -> Vec<TextToken> {
        self.process_query_term(term)
            .into_iter()
            .map(|(position, text)| TextToken {
                position,
                text: text.into(),
            })
            .collect()
    }
}

impl Processor for EngineProcessor {
    fn process_document_tokens(&self, value: &str) -> Vec<(usize, Box<str>)> {
        self.inner.processor.process_document_tokens(value)
    }

    fn process_query_term(&self, term: &str) -> Vec<(usize, Box<str>)> {
        self.inner.processor.process_query_term(term)
    }
}

/// Text token tracking its position within the original string of text
#[wasm_bindgen]
pub struct TextToken {
    position: usize,
    text: String,
}

#[wasm_bindgen]
impl TextToken {
    /// Create a new TextToken
    #[wasm_bindgen(constructor)]
    pub fn new(position: usize, text: String) -> Self {
        Self { position, text }
    }
    /// Get the token text
    pub fn text(&self) -> String {
        self.text.clone()
    }
    /// Get the token position in original text
    pub fn position(&self) -> usize {
        self.position
    }
}

#[wasm_bindgen]
impl Write {
    /// Prepares the worker for inserting a new document when committing.
    #[wasm_bindgen(js_name = "insert")]
    pub fn insert_wasm(&mut self, document: Document) {
        self.insert(document)
    }
}

#[wasm_bindgen]
impl Execution {
    /// Fetch the next event
    #[wasm_bindgen(js_name = "next")]
    pub fn next_wasm(&mut self) -> Option<WriteEvent> {
        self.next().map(WriteEvent)
    }
}

#[wasm_bindgen]
impl Cleanup {
    /// Fetch the next event
    #[wasm_bindgen(js_name = "next")]
    pub fn next_wasm(&mut self) -> Option<CleanupEvent> {
        self.next().map(CleanupEvent)
    }
}

#[wasm_bindgen]
impl Search {
    /// Fetch the next event
    #[wasm_bindgen(js_name = "next")]
    pub fn next_wasm(&mut self) -> Option<QueryEvent> {
        self.next().map(QueryEvent)
    }
}

#[wasm_bindgen]
impl Query {
    /// Parse a query from string and add it to the query expression
    #[wasm_bindgen(js_name = "withStringExpression")]
    pub fn with_string_expression(self, query: &str) -> Result<Self, JsError> {
        Ok(self.with_expression(query.parse().map_err(JsError::from)?))
    }

    #[wasm_bindgen(js_name = "withStructuredExpression")]
    /// Add a structured query expression
    pub fn with_structured_expression(self, query: Expression) -> Self {
        self.with_expression(query.into())
    }
}

/// Search engine write event
#[wasm_bindgen]
pub struct WriteEvent(super::WriteEvent);

/// The kind of the engine write event
#[wasm_bindgen]
pub enum WriteEventKind {
    /// Load event
    Load,
    /// Save event
    Save,
    /// Stats event
    Stats,
}

#[wasm_bindgen]
impl WriteEvent {
    /// Get the event kind
    #[wasm_bindgen]
    pub fn kind(&self) -> WriteEventKind {
        match &self.0 {
            super::WriteEvent::Load(..) => WriteEventKind::Load,
            super::WriteEvent::Save(..) => WriteEventKind::Save,
            super::WriteEvent::Stats(..) => WriteEventKind::Stats,
        }
    }

    /// Blob name for load/save events
    ///
    /// You should check first that this is indeed a load/save event. If not, this call will error.
    #[wasm_bindgen]
    pub fn id(&self) -> Result<BlobId, JsError> {
        match &self.0 {
            super::WriteEvent::Load(event) => Ok(event.id()),
            super::WriteEvent::Save(event) => Ok(event.id()),
            super::WriteEvent::Stats(_) => Err(JsError::new("Not a load/save event")),
        }
    }

    /// Get cached value in case of Save event
    ///
    /// You should check first that this is indeed a save event. If not, this call will error.
    #[wasm_bindgen]
    pub fn recv(self) -> Result<Cached, JsError> {
        match self.0 {
            super::WriteEvent::Save(save_event) => Ok(save_event.recv()),
            super::WriteEvent::Load(_) | super::WriteEvent::Stats(_) => {
                Err(JsError::new("Not a save event"))
            }
        }
    }

    /// Invoke the load callback, passing a serialized value.
    ///
    /// You should check first that this is indeed a load event. If not, this call will error.
    #[wasm_bindgen]
    pub fn send(self, serdes: SerDes, data: Vec<u8>) -> Result<Cached, JsError> {
        let load_event = match self.0 {
            super::WriteEvent::Load(load_event) => load_event,
            super::WriteEvent::Save(_) | super::WriteEvent::Stats(_) => {
                return Err(JsError::new("Not a load event"));
            }
        };
        send(load_event, &serdes, data)
    }

    /// Invoke the load callback, passing a cached value.
    ///
    /// You should check first that this is indeed a load event. If not, this call will error.
    #[wasm_bindgen(js_name = "sendCached")]
    pub fn send_cached(self, cached: &Cached) -> Result<(), JsError> {
        let load_event = match self.0 {
            super::WriteEvent::Load(load_event) => load_event,
            super::WriteEvent::Save(_) | super::WriteEvent::Stats(_) => {
                return Err(JsError::new("Not a load event"));
            }
        };
        send_cached(load_event, cached)
    }

    /// Invoke the load callback, passing no value.
    ///
    /// You should check first that this is indeed a load event. If not, this call will error.
    #[wasm_bindgen(js_name = "sendEmpty")]
    pub fn send_empty(self) -> Result<Cached, JsError> {
        let load_event = match self.0 {
            super::WriteEvent::Load(load_event) => load_event,
            super::WriteEvent::Save(_) | super::WriteEvent::Stats(_) => {
                return Err(JsError::new("Not a load event"));
            }
        };
        send_empty(load_event)
    }

    /// Get the stats
    ///
    /// You should check first that this is indeed a stats event. If not, this call will error.
    #[wasm_bindgen]
    pub fn stats(self) -> Result<Stats, JsError> {
        match self.0 {
            super::WriteEvent::Stats(stats) => Ok(stats),
            super::WriteEvent::Save(_) | super::WriteEvent::Load(_) => {
                Err(JsError::new("Not a stats event"))
            }
        }
    }
}

/// Search engine cleanup event
#[wasm_bindgen]
pub struct CleanupEvent(super::CleanupEvent);

/// The kind of the engine write event
#[wasm_bindgen]
pub enum CleanupEventKind {
    /// Load event
    Load,
    /// Save event
    Save,
    /// Blob release event
    Release,
}

#[wasm_bindgen]
impl CleanupEvent {
    /// Get the event kind
    #[wasm_bindgen]
    pub fn kind(&self) -> CleanupEventKind {
        match &self.0 {
            super::CleanupEvent::Release(..) => CleanupEventKind::Release,
            super::CleanupEvent::Save(..) => CleanupEventKind::Save,
            super::CleanupEvent::Load(..) => CleanupEventKind::Load,
        }
    }

    /// Blob name for load/save events
    #[wasm_bindgen]
    pub fn id(&self) -> BlobId {
        match &self.0 {
            super::CleanupEvent::Release(event) => event.id(),
            super::CleanupEvent::Save(event) => event.id(),
            super::CleanupEvent::Load(event) => event.id(),
        }
    }

    /// Invoke the load callback.
    ///
    /// You should check first that this is indeed a load event. If not, this call will error.
    #[wasm_bindgen]
    pub fn send(self, serdes: SerDes, data: Vec<u8>) -> Result<Cached, JsError> {
        let load_event = match self.0 {
            super::CleanupEvent::Load(load_event) => load_event,
            super::CleanupEvent::Save(..) | super::CleanupEvent::Release(..) => {
                return Err(JsError::new("Not a load event"));
            }
        };
        send(load_event, &serdes, data)
    }

    /// Invoke the load callback, passing a cached value.
    ///
    /// You should check first that this is indeed a load event. If not, this call will error.
    #[wasm_bindgen(js_name = "sendCached")]
    pub fn send_cached(self, cached: &Cached) -> Result<(), JsError> {
        let load_event = match self.0 {
            super::CleanupEvent::Load(load_event) => load_event,
            super::CleanupEvent::Save(_) | super::CleanupEvent::Release(_) => {
                return Err(JsError::new("Not a load event"));
            }
        };
        send_cached(load_event, cached)
    }

    /// Invoke the load callback, passing no value.
    ///
    /// You should check first that this is indeed a load event. If not, this call will error.
    #[wasm_bindgen(js_name = "sendEmpty")]
    pub fn send_empty(self) -> Result<Cached, JsError> {
        let load_event = match self.0 {
            super::CleanupEvent::Load(load_event) => load_event,
            super::CleanupEvent::Save(_) | super::CleanupEvent::Release(_) => {
                return Err(JsError::new("Not a load event"));
            }
        };
        send_empty(load_event)
    }

    /// Get cached value in case of Save event
    ///
    /// You should check first that this is indeed a save event. If not, this call will error.
    #[wasm_bindgen]
    pub fn recv(self) -> Result<Cached, JsError> {
        match self.0 {
            super::CleanupEvent::Save(save_event) => Ok(save_event.recv()),
            super::CleanupEvent::Release(_) | super::CleanupEvent::Load(_) => {
                Err(JsError::new("Not a save event"))
            }
        }
    }
}

/// Search engine query event
#[wasm_bindgen]
pub struct QueryEvent(super::query::QueryEvent);

/// The kind of the engine write event
#[wasm_bindgen]
pub enum QueryEventKind {
    /// Load event
    Load,
    /// Found matching entry event
    Found,
    /// Statistics collected
    Stats,
}

#[wasm_bindgen]
impl QueryEvent {
    /// Get the event kind
    #[wasm_bindgen]
    pub fn kind(&self) -> QueryEventKind {
        match &self.0 {
            super::QueryEvent::Found(..) => QueryEventKind::Found,
            super::QueryEvent::Load(..) => QueryEventKind::Load,
            super::QueryEvent::Stats(..) => QueryEventKind::Stats,
        }
    }

    /// Blob name for load/save events
    ///
    /// You should check first that this is indeed a load/save event. If not, this call will error.
    #[wasm_bindgen]
    pub fn id(&self) -> Result<BlobId, JsError> {
        match &self.0 {
            super::QueryEvent::Load(load) => Ok(load.id()),
            super::QueryEvent::Found(..) | super::QueryEvent::Stats(..) => {
                Err(JsError::new("Not a load/save event"))
            }
        }
    }

    /// Stats collected for this search
    #[wasm_bindgen]
    pub fn stats(self) -> Option<CollectionStats> {
        match self.0 {
            super::QueryEvent::Found(..) => None,
            super::QueryEvent::Load(..) => None,
            super::QueryEvent::Stats(stats) => Some(stats),
        }
    }

    /// Found entry
    #[wasm_bindgen]
    pub fn found(self) -> Option<FoundEntry> {
        match self.0 {
            super::QueryEvent::Found(found) => Some(found),
            super::QueryEvent::Load(..) => None,
            super::QueryEvent::Stats(..) => None,
        }
    }

    /// Invoke the load callback.
    ///
    /// You should check first that this is indeed a load event. If not, this call will error.
    #[wasm_bindgen]
    pub fn send(self, serdes: SerDes, data: Vec<u8>) -> Result<Cached, JsError> {
        let load_event = match self.0 {
            super::QueryEvent::Load(load_event) => load_event,
            super::QueryEvent::Found(..) | super::QueryEvent::Stats(..) => {
                return Err(JsError::new("Not a load event"));
            }
        };
        send(load_event, &serdes, data)
    }

    /// Invoke the load callback, passing a cached value.
    ///
    /// You should check first that this is indeed a load event. If not, this call will error.
    #[wasm_bindgen(js_name = "sendCached")]
    pub fn send_cached(self, cached: &Cached) -> Result<(), JsError> {
        let load_event = match self.0 {
            super::QueryEvent::Load(load_event) => load_event,
            super::QueryEvent::Found(_) | super::QueryEvent::Stats(_) => {
                return Err(JsError::new("Not a load event"));
            }
        };
        send_cached(load_event, cached)
    }

    /// Invoke the load callback.
    ///
    /// You should check first that this is indeed a load event. If not, this call will error.
    #[wasm_bindgen(js_name = "sendEmpty")]
    pub fn send_empty(self) -> Result<Cached, JsError> {
        let load_event = match self.0 {
            super::QueryEvent::Load(load_event) => load_event,
            super::QueryEvent::Found(..) | super::QueryEvent::Stats(..) => {
                return Err(JsError::new("Not a load event"));
            }
        };
        send_empty(load_event)
    }
}

fn send(load_event: LoadEvent, serdes: &SerDes, data: Vec<u8>) -> Result<Cached, JsError> {
    load_event
        .send(serdes, &data)
        .map_err(|e| Error(e.to_string()).into())
}

fn send_empty(load_event: LoadEvent) -> Result<Cached, JsError> {
    load_event
        .send_empty()
        .map_err(|e| Error(e.to_string()).into())
}

fn send_cached(load_event: LoadEvent, cached: &Cached) -> Result<(), JsError> {
    load_event
        .send_cached(cached)
        .map_err(|e| Error(e.to_string()).into())
}

#[derive(Debug)]
struct Error(String);
impl std::error::Error for Error {}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        self.0.fmt(f)
    }
}
