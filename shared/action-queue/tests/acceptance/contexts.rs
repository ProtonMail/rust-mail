use super::common::DefaultError;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_action_queue::rebase::RebaseChangeSet;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use std::marker::PhantomData;

#[derive(Serialize, Deserialize)]
struct Action1 {}

impl Action for Action1 {
    const TYPE: Type = Type("Action1");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = ActionHandler<Self>;
    type RemoteOutput = u32;
    type LocalOutput = ();
    type Error = DefaultError;
}

#[derive(Serialize, Deserialize)]
struct Action2 {}

impl Action for Action2 {
    const TYPE: Type = Type("Action2");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = ActionHandler<Self>;
    type RemoteOutput = u32;
    type LocalOutput = ();
    type Error = DefaultError;
}

struct ActionHandler<A>(PhantomData<A>);

impl<A> Default for ActionHandler<A> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<A> Handler for ActionHandler<A>
where
    A: Action<Handler = Self, LocalOutput: Default, RemoteOutput: Default> + Send + Sync,
{
    type Action = A;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        Ok(<Self::Action as Action>::LocalOutput::default())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        Ok(<Self::Action as Action>::RemoteOutput::default())
    }
    async fn rebase_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &RebaseChangeSet,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }
}
