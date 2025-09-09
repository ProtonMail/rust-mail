use crate::common::prelude::*;
use crate::http::{HttpReq, HttpRes};

/// A dynamic HTTP connector.
pub type DynHttpConnector = DynConnector<HttpReq, HttpRes>;

/// A dynamic HTTP connector layer.
pub type DynHttpConnectorLayer = DynConnectorLayer<HttpReq, HttpRes>;

/// A dynamic, bound HTTP connector.
pub type DynBoundHttpConnector = DynBoundConnector<HttpReq, HttpRes>;

/// A dynamic HTTP sender.
pub type DynHttpSender = DynSender<HttpReq, HttpRes>;

/// A dynamic HTTP sender layer.
pub type DynHttpSenderLayer = DynSenderLayer<HttpReq, HttpRes>;
