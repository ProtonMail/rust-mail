use crate::test::server::Request;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, Weak};

/// Holds all registered message recorders for a server.
#[derive(Debug, Default, Clone)]
pub struct Recorder {
    tx: Arc<Mutex<Vec<Tx>>>,
}

impl Recorder {
    /// Create a new recorder pair.
    pub fn new_recorder(&self) -> Arc<Rx> {
        let rx = Rx::new();
        let tx = Tx::new(&rx);

        self.tx.lock().unwrap().push(tx);

        rx
    }

    /// Push a message to all recorders.
    pub fn push(&self, msg: &Request<Vec<u8>>) {
        self.tx.lock().unwrap().retain(|tx| tx.push(msg.to_owned()));
    }
}

/// The "sender" side of a single recorder.
#[derive(Debug)]
pub struct Tx(Weak<Rx>);

/// The "receiver" side of a single recorder.
#[derive(Debug, Default)]
pub struct Rx(Mutex<VecDeque<Request<Vec<u8>>>>);

impl Tx {
    fn new(rx: &Arc<Rx>) -> Self {
        Self(Arc::downgrade(rx))
    }

    pub fn push(&self, msg: Request<Vec<u8>>) -> bool {
        self.0.upgrade().map(|rx| rx.push(msg)).is_some()
    }
}

impl Rx {
    fn new() -> Arc<Self> {
        Arc::default()
    }

    fn push(&self, msg: Request<Vec<u8>>) {
        self.0.lock().unwrap().push_back(msg);
    }

    pub fn read(&self) -> VecDeque<Request<Vec<u8>>> {
        self.0.lock().unwrap().clone()
    }

    pub fn take(&self) -> VecDeque<Request<Vec<u8>>> {
        self.0.lock().unwrap().drain(..).collect()
    }
}
