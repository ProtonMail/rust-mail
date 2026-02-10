//! This example demonstrates adding additional trust anchors to the client.

use anyhow::Result;
use muon::tls::{TlsCertDer, TrustAnchor};
use muon::util::ProtonRequestExt;
use muon::{App, Client, GET};

/// A custom environment.
struct MyTrustAnchor {
    root: TlsCertDer,
}

impl MyTrustAnchor {
    fn new() -> Self {
        Self {
            root: Vec::from(b"... data here ..."),
        }
    }
}

/// Implement [`TrustAnchor`] to specify the trust anchor(s) to add.
impl TrustAnchor for MyTrustAnchor {
    fn roots(&self) -> Vec<&TlsCertDer> {
        vec![&self.root]
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // First, define which app is using the client.
    let app = App::new("windows-vpn@4.1.0")?.with_user_agent("Mozilla/5.0");
    let store = muon::env::EnvId::new_atlas();
    let anchor = MyTrustAnchor::new();
    let client = Client::builder(app, store).anchor(anchor).build()?;

    GET!("/").send_with(&client).await?.ok()?;

    Ok(())
}
