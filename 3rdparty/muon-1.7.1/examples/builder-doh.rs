//! This example demonstrates how to configure DNS-over-HTTPS services.

use anyhow::Result;
use muon::dns::{CloudflareDoh, DohService, GoogleDoh, Quad9Doh};
use muon::{App, Client};
/// Define a custom DNS-over-HTTPS service.
struct MyDohSvc;

// Specify the URL for the custom DNS-over-HTTPS service.
impl DohService for MyDohSvc {
    fn server(&self) -> muon::common::Server {
        "https://foo.bar/dns-query".parse().unwrap()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let app = App::new("windows-vpn@4.1.0")?;
    let store = muon::env::EnvId::new_atlas();

    // Create the client, setting the DNS-over-HTTPS services to use.
    let _ = Client::builder(app, store)
        .doh([GoogleDoh])
        .doh([CloudflareDoh])
        .doh([Quad9Doh])
        .doh([MyDohSvc])
        .build()?;

    Ok(())
}
