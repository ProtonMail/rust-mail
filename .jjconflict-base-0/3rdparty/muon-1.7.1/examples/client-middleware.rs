//! This example demonstrates how to add logging layers to the client.
//!
//! A muon client can be extended by adding additional layers either at the
//! "front" or "back" of the client's sender stack. A layer is a type
//! implementing [`SenderLayer`], and this example shows both pre-defined and
//! custom layers.

use anyhow::Result;
use muon::client::middleware::{DisplayLogger, Status429Handler, Status5xxHandler};
use muon::common::{BoxFut, Sender, SenderLayer};
use muon::util::ProtonRequestExt;
use muon::{App, Client, Error, ProtonRequest, ProtonResponse, GET};
/// A custom logging layer that just prints stuff to stdout.
struct MyCustomLogger;

impl SenderLayer<ProtonRequest, ProtonResponse> for MyCustomLogger {
    fn on_send<'a>(
        &'a self,
        inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
        req: ProtonRequest,
    ) -> BoxFut<'a, muon::Result<ProtonResponse>> {
        Box::pin(async move {
            println!("sending {req}");
            let res = inner.send(req).await?;
            println!("received {res}");

            Ok(res)
        })
    }
}

/// A custom retry layer that naively retries Protonrequests up to 3 times.
struct MySillyRetryLayer;

impl SenderLayer<ProtonRequest, ProtonResponse> for MySillyRetryLayer {
    fn on_send<'a>(
        &'a self,
        inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
        req: ProtonRequest,
    ) -> BoxFut<'a, muon::Result<ProtonResponse>> {
        Box::pin(async move {
            for _ in 0..3 {
                match inner.send(req.clone()).await {
                    Ok(res) if res.status().is_success() => {
                        println!("success: {res}");
                        return Ok(res);
                    }

                    Ok(res) => {
                        eprintln!("ProtonResponse indicates failure: {res}");
                    }

                    Err(e) => {
                        eprintln!("Protonrequest could not be sent: {e}");
                    }
                }
            }

            Err(Error::other("oops"))
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let app = App::new("windows-vpn@4.1.0")?;
    let store = muon::env::EnvId::new_atlas();
    let client = Client::builder(app, store)
        .layer_back(MyCustomLogger) // custom logger
        .layer_back(Status429Handler) // pre-defined layer
        .layer_back(Status5xxHandler) // pre-defined layer
        .layer_back(MySillyRetryLayer) // custom retry layer
        .layer_front(DisplayLogger::debug()) // pre-defined layer
        .build()?;

    GET!("/tests/ping").send_with(&client).await?.ok()?;

    Ok(())
}
