//! Request child data structures for the Proton API.
//!
//! This module provides child data types that are used by the request
//! structures when sending requests to the Proton API.
//!
//! The purpose of the API service is to provide not only the means to make
//! requests, but also a formalisation of the data that is sent and received. To
//! this end, the structs in this module should mirror the API endpoint request
//! definitions, and NOT have any business logic or other functionality.
//!
//! Structs in this module should only implement [`Serialize`], and should not
//! implement [`Deserialize`](serde::Deserialize). If anything in this module
//! implements [`Deserialize`](serde::Deserialize), it is a sign that a mistake
//! has been made.
//!
//! Any types that used by both requests and responses should be defined in the
//! [`common`](crate::services::proton::common) module.
//!
