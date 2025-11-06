use crate::action::{Action, ActionId, Error, Handler, WriterGuard, WriterGuardError};
use crate::queue::ActionRequeueReason;
use crate::rebase::RebaseChangeSet;
use stash::stash::{Bond, StashError};
use std::marker::PhantomData;

pub struct NoopActionHandler<T: Action>(PhantomData<T>);

impl<T: Action> Default for NoopActionHandler<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T> Handler for NoopActionHandler<T>
where
    T: Action<Handler = Self, LocalOutput: Default, RemoteOutput: Default> + Send + Sync,
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

    async fn revert_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), T::Error> {
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<<T as Action>::RemoteOutput, T::Error> {
        Ok(T::RemoteOutput::default())
    }

    async fn rebase_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &RebaseChangeSet,
        _: &Bond<'_>,
    ) -> Result<(), T::Error> {
        Ok(())
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
    fn can_requeue(&self) -> Option<ActionRequeueReason> {
        match self {
            DefaultError::NetworkFailure => Some(ActionRequeueReason::NetworkFailed),
            DefaultError::WriterGuardExpired => Some(ActionRequeueReason::GuardExpired),
            _ => None,
        }
    }
}
