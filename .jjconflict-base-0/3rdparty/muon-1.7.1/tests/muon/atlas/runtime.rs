use crate::atlas::new_builder;
use anyhow::Result;
use muon::rt::PollWith;
use muon::GET;

#[tokio::test]
async fn test_runtime_dispatcher() -> Result<()> {
    let builder = new_builder();

    // Create a dispatcher and its driver.
    let (dispatcher, driver) = muon::rt::dispatcher();

    // Spawn the driver onto the runtime.
    tokio::spawn(driver);

    // Create a client using the dispatcher.
    let c = builder.spawner(dispatcher).build()?;

    // This future will be executed by the dispatcher.
    c.send(GET!("/tests/ping")).await?.ok()?;

    Ok(())
}

#[tokio::test]
async fn test_runtime_dispatcher_poll_with() -> Result<()> {
    let builder = new_builder();

    // Create a dispatcher and its driver.
    // We don't spawn the driver onto the runtime here.
    let (dispatcher, driver) = muon::rt::dispatcher();

    // Create a client using the dispatcher.
    let c = builder.spawner(dispatcher).build()?;

    // This future will be executed by the current thread,
    // which will also drive the dispatcher.
    c.send(GET!("/tests/ping")).poll_with(&driver).await?.ok()?;

    Ok(())
}

#[tokio::test]
#[cfg(feature = "rt-tokio")]
async fn test_runtime_tokio() -> Result<()> {
    use muon::rt::{TokioDialer, TokioResolver, TokioSpawner};

    let c = new_builder()
        .resolver(TokioResolver)
        .dialer(TokioDialer)
        .spawner(TokioSpawner)
        .build()?;

    c.send(GET!("/tests/ping")).await?.ok()?;

    Ok(())
}
