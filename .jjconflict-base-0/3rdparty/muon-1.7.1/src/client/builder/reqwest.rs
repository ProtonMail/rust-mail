use crate::client::builder::{BaseBuilder, Transport};
use crate::common::*;
use crate::env::DynEnv;
use crate::http::*;
use crate::{App, Result};

/// A [`BaseBuilder`] for configuring a [`Reqwest`] transport.
pub type ReqwestBuilder = BaseBuilder<Reqwest>;

/// A [`Transport`] that builds a [`reqwest`]-based connector.
#[derive(Debug, Default)]
pub struct Reqwest(());

impl Transport for Reqwest {
    fn build(self, _: &DynEnv) -> Result<DynHttpConnector> {
        Ok(ReqwestConnector.into_dyn())
    }
}

if_sealed! {
    impl crate::Sealed for Reqwest {}
}
