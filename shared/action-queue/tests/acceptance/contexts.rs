use super::common::DefaultError;
use super::common::new_queue_typed;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use std::any::Any;
use std::marker::PhantomData;
use std::sync::Arc;

#[tokio::test]
async fn actions_with_different_contexts() -> Result<(), anyhow::Error> {
    // Check that if remote fails to execute when action is applied, local state is reverted.
    let queue = new_queue_typed::<Action1>().await;
    queue.register::<Action2>()?;

    let context1 = Arc::new(true);
    let context2 = Arc::new(1024_usize);

    queue.register_execution_context(Arc::downgrade(&context1));
    queue.register_execution_context(Arc::downgrade(&context2));

    queue.queue_action(Action1 {}).await?;
    queue.queue_action(Action2 {}).await?;

    queue.new_executor().execute_all().await?;

    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Action1 {}

#[derive(Serialize, Deserialize)]
struct Action2 {}

impl Action for Action1 {
    const TYPE: Type = Type("Action1");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = ActionHandler<Self::Context, Self>;
    type RemoteOutput = u32;

    type LocalOutput = ();
    type Error = DefaultError;
    type Context = bool;
}

impl Action for Action2 {
    const TYPE: Type = Type("Action2");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = ActionHandler<Self::Context, Self>;
    type RemoteOutput = u32;

    type LocalOutput = ();
    type Error = DefaultError;
    type Context = usize;
}

struct ActionHandler<C, A: Action>(PhantomData<fn() -> (C, A)>);

impl<C, A: Action> Default for ActionHandler<C, A> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<C: Any + Send + Sync + 'static, A: Action> Handler for ActionHandler<C, A>
where
    <A as Action>::LocalOutput: Default,
    <A as Action>::RemoteOutput: Default,
{
    type Action = A;
    type Context = C;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        Ok(<Self::Action as Action>::LocalOutput::default())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        Ok(<Self::Action as Action>::RemoteOutput::default())
    }
}
