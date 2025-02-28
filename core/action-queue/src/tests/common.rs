#![allow(non_snake_case)]
#![allow(clippy::ignored_unit_patterns)]

use crate::action::{Action, ActionId, Handler, WriterGuard};
use crate::network::WaitForOnline;
use stash::stash::Bond;
use std::future::Future;
use std::marker::PhantomData;

pub(crate) struct DummyWaitForOnline;
#[async_trait::async_trait]
impl WaitForOnline for DummyWaitForOnline {
    async fn wait_for_online(&self) {}
}

pub(crate) struct NoopActionHandler<T: Action>(PhantomData<T>);

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
    type Context = ();

    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<<T as Action>::LocalOutput, T::Error> {
        Ok(<T as Action>::LocalOutput::default())
    }

    fn revert_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> impl Future<Output = Result<(), T::Error>> + Send {
        std::future::ready(Ok(()))
    }

    fn apply_remote(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: WriterGuard,
    ) -> impl Future<Output = Result<<T as Action>::RemoteOutput, T::Error>> + Send {
        std::future::ready(Ok(T::RemoteOutput::default()))
    }
}
