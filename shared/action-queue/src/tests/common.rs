#![allow(non_snake_case)]
#![allow(clippy::ignored_unit_patterns)]

use crate::action::{Action, ActionId, Error, Handler, WriterGuard, WriterGuardError};
use stash::stash::{Bond, StashError};
use std::future::Future;
use std::marker::PhantomData;

pub struct NoopActionHandler<T: Action>(PhantomData<T>);

impl<T: Action> Default for NoopActionHandler<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: Action + 'static + Sync> Handler for NoopActionHandler<T>
where
    <T as Action>::RemoteOutput: Default + Send,
    <T as Action>::LocalOutput: Default,
{
    type Action = T;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<<T as Action>::LocalOutput, T::Error> {
        Ok(<T as Action>::LocalOutput::default())
    }

    fn revert_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> impl Future<Output = Result<(), T::Error>> + Send {
        std::future::ready(Ok(()))
    }

    fn apply_remote(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: WriterGuard,
    ) -> impl Future<Output = Result<<T as Action>::RemoteOutput, T::Error>> + Send {
        std::future::ready(Ok(T::RemoteOutput::default()))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DefaultError {
    #[error("Network Failure")]
    NetworkFailure,
    #[error("API Failure")]
    APIFailure,
    #[error("{0}")]
    Other(anyhow::Error),
    #[error("{0}")]
    DB(#[from] StashError),
    #[error("Writer Guard Expired")]
    WriterGuardExpired,
}

impl From<WriterGuardError> for DefaultError {
    fn from(value: WriterGuardError) -> Self {
        match value {
            WriterGuardError::Expired => Self::WriterGuardExpired,
            WriterGuardError::Stash(e) => Self::DB(e),
        }
    }
}

impl Error for DefaultError {
    fn is_network_failure(&self) -> bool {
        matches!(self, DefaultError::NetworkFailure)
    }

    fn is_writer_guard_expired(&self) -> bool {
        matches!(self, DefaultError::WriterGuardExpired)
    }
}
