#![allow(non_snake_case)]

use crate::action::{Action, Handler};
use proton_api_core::session::Session;
use stash::stash::{Stash, Tether};
use std::marker::PhantomData;

pub(crate) struct NoopActionHandler<T: Action>(PhantomData<T>);

impl<T: Action> Default for NoopActionHandler<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: Action + 'static> Handler for NoopActionHandler<T>
where
    <T as Action>::Output: Default,
{
    type Action = T;

    async fn apply_local(&self, _: &mut Self::Action, _: &Tether) -> Result<(), T::Error> {
        Ok(())
    }

    async fn revert_local(&self, _: &mut Self::Action, _: &Tether) -> Result<(), T::Error> {
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: &mut Self::Action,
        _: &Session,
        _: &Stash,
    ) -> Result<<T as Action>::Output, T::Error> {
        Ok(T::Output::default())
    }
}
