use serde::{Deserialize, Serialize};

/// `GET /tests/ping`
#[derive(Debug, Serialize, Deserialize)]
pub struct Get;
