use axum::extract::Request;
use axum::response::Response;
use derive_more::Debug;
use muon_proc::autoimpl;
use std::sync::{Arc, Mutex};

/// A request handler.
pub type Handler = Box<dyn Fn(&Request) -> Option<Response> + Send + Sync + 'static>;

/// Holds all registered responders for a server.
#[derive(Debug, Default, Clone)]
#[debug("Responder")]
pub struct Responder {
    handler: Arc<Mutex<Vec<Handler>>>,
}

impl Responder {
    /// Push a request matcher to the responder.
    pub fn push(&self, handler: Handler) {
        self.handler.lock().unwrap().push(handler);
    }

    /// Get the response for a request.
    pub fn get(&self, req: &Request) -> Option<Response> {
        let mut handler = self.handler.lock().unwrap();

        let (idx, res) = handler.iter().find_position_map(|h| h(req))?;

        drop(handler.remove(idx));

        Some(res)
    }
}

/// find_position_map: a mix of find_map and find_position
#[autoimpl]
trait FindPositionMap: IntoIterator + Sized {
    fn find_position_map<T, F>(self, f: F) -> Option<(usize, T)>
    where
        F: Fn(&Self::Item) -> Option<T>,
    {
        self.into_iter()
            .enumerate()
            .find_map(|(i, x)| f(&x).map(|y| (i, y)))
    }
}
