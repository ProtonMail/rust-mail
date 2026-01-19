//! Explicitly WASM specific implementations and types

use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::document::Document;
use crate::engine::{Cleanup, Execution, Query, Search, Write};
use crate::query::expression::wasm::Expression;
use crate::query::results::FoundEntry;
use crate::query::stats::CollectionStats;
use crate::serialization::SerDes;
use crate::transaction::{LoadEvent, SaveEvent};

#[wasm_bindgen]
impl Write {
    /// Prepares the worker for inserting a new document when committing.
    #[wasm_bindgen(js_name = "insert")]
    pub fn insert_wasm(&mut self, document: Document) -> Result<(), JsError> {
        self.insert(document).map_err(JsError::from)
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
    /// Modified event
    Modified,
}

#[wasm_bindgen]
impl WriteEvent {
    /// Get the event kind
    #[wasm_bindgen]
    pub fn kind(&self) -> WriteEventKind {
        match &self.0 {
            super::WriteEvent::Modified(..) => WriteEventKind::Modified,
            super::WriteEvent::Load(..) => WriteEventKind::Load,
            super::WriteEvent::Save(..) => WriteEventKind::Save,
        }
    }

    /// Blob name for load/save events
    #[wasm_bindgen]
    pub fn name(&self) -> String {
        match &self.0 {
            super::WriteEvent::Modified(name)
            | super::WriteEvent::Load(LoadEvent { name, .. })
            | super::WriteEvent::Save(SaveEvent { name, .. }) => name.to_string(),
        }
    }

    /// Invoke the save callback.
    ///
    /// You should check first that this is indeed a save event. If not, this call will error.
    #[wasm_bindgen]
    pub fn recv(self, serdes: SerDes) -> Result<Vec<u8>, JsError> {
        let save_event = match self.0 {
            super::WriteEvent::Save(save_event) => Some(save_event),
            super::WriteEvent::Load(_) | super::WriteEvent::Modified(..) => {
                return Err(JsError::new("Not a save event"));
            }
        };
        recv(save_event, &serdes)
    }

    /// Invoke the load callback.
    ///
    /// You should check first that this is indeed a load event. If not, this call will error.
    #[wasm_bindgen]
    pub fn send(self, serdes: SerDes, data: Vec<u8>) -> Result<(), JsError> {
        let load_event = match self.0 {
            super::WriteEvent::Load(load_event) => Some(load_event),
            super::WriteEvent::Save(_) | super::WriteEvent::Modified(..) => {
                return Err(JsError::new("Not a load event"));
            }
        };
        send(load_event, &serdes, data)
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
    pub fn name(&self) -> String {
        match &self.0 {
            super::CleanupEvent::Release(name)
            | super::CleanupEvent::Save(SaveEvent { name, .. })
            | super::CleanupEvent::Load(LoadEvent { name, .. }) => name.to_string(),
        }
    }

    /// Invoke the load callback.
    ///
    /// You should check first that this is indeed a load event. If not, this call will error.
    #[wasm_bindgen]
    pub fn send(self, serdes: SerDes, data: Vec<u8>) -> Result<(), JsError> {
        let load_event = match self.0 {
            super::CleanupEvent::Load(load_event) => Some(load_event),
            super::CleanupEvent::Save(..) | super::CleanupEvent::Release(..) => {
                return Err(JsError::new("Not a load event"));
            }
        };
        send(load_event, &serdes, data)
    }

    /// Invoke the save callback.
    ///
    /// You should check first that this is indeed a save event. If not, this call will error.
    #[wasm_bindgen]
    pub fn recv(self, serdes: SerDes) -> Result<Vec<u8>, JsError> {
        let save_event = match self.0 {
            super::CleanupEvent::Save(save_event) => Some(save_event),
            super::CleanupEvent::Load(_) | super::CleanupEvent::Release(..) => {
                return Err(JsError::new("Not a save event"));
            }
        };
        recv(save_event, &serdes)
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
    #[wasm_bindgen]
    pub fn name(&self) -> String {
        match &self.0 {
            super::QueryEvent::Found(found) => found.identifier().to_string(),
            super::QueryEvent::Load(LoadEvent { name, .. }) => name.to_string(),
            super::QueryEvent::Stats(..) => String::new(),
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
    pub fn send(self, serdes: SerDes, data: Vec<u8>) -> Result<(), JsError> {
        let load_event = match self.0 {
            super::QueryEvent::Load(load_event) => Some(load_event),
            super::QueryEvent::Found(..) | super::QueryEvent::Stats(..) => {
                return Err(JsError::new("Not a load event"));
            }
        };
        send(load_event, &serdes, data)
    }
}

fn send(load_event: Option<LoadEvent>, serdes: &SerDes, data: Vec<u8>) -> Result<(), JsError> {
    #[derive(Debug)]
    struct Error(String);
    impl std::error::Error for Error {}
    impl std::fmt::Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
            self.0.fmt(f)
        }
    }
    match load_event {
        None => Err(Error("called a load on a non-load event".into()).into()),
        Some(LoadEvent { send, .. }) => send(serdes, data).map_err(|e| Error(e.to_string()).into()),
    }
}

fn recv(save_event: Option<SaveEvent>, serdes: &SerDes) -> Result<Vec<u8>, JsError> {
    #[derive(Debug)]
    struct Error(String);
    impl std::error::Error for Error {}
    impl std::fmt::Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
            self.0.fmt(f)
        }
    }
    match save_event {
        None => Err(Error("called a load on a non-load event".into()).into()),
        Some(SaveEvent { recv, .. }) => {
            recv(serdes).map_err(|e| JsError::from(Error(e.to_string())))
        }
    }
}
