//! Simple infinite concurrent ping loop.

use anyhow::Result;
use futures::future::pending;
use muon::test::store::TestStore;
use muon::util::DurationExt;
use muon::{App, Client, GET};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use tokio::time::sleep;

const CLIENTS: usize = 20;
const N_TASKS: usize = 20;
const N_PINGS: usize = 2;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    for _ in 0..CLIENTS {
        let c = new_client()?;

        for _ in 0..N_TASKS {
            let c = c.clone();

            tokio::spawn(async move {
                let r = StdRng::from_entropy();

                ping_loop(c, r).await;
            });
        }
    }

    pending().await
}

async fn ping_loop(c: Client, mut r: StdRng) {
    loop {
        let d = r.gen_range(0..CLIENTS * N_TASKS / N_PINGS);

        sleep(d.s()).await;

        let _ = c.send(GET!("/tests/ping")).await;
    }
}

fn new_client() -> Result<Client> {
    let app = App::new("ios-mail@7.1.0")?;
    let store = TestStore::prod();
    let client = Client::new(app, store)?;

    Ok(client)
}
