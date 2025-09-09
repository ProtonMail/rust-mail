//! This example demonstrates how to configure DNS services.

use anyhow::Result;
use muon::dns::{CloudflareDns, DnsService, GoogleDns, Quad9Dns};
use muon::{App, Client};

/// Define a custom DNS service.
struct MyDnsSvc;

// Specify the URL for the custom DNS-over-HTTPS service.
impl DnsService for MyDnsSvc {
    fn ip(&self) -> ::std::net::IpAddr {
        [1, 2, 3, 4].into()
    }

    fn port(&self) -> u16 {
        53
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let app = App::new("windows-vpn@4.1.0")?;
    let store = muon::env::EnvId::new_atlas();

    // Create the client, setting the DNS services to use.
    let _ = Client::builder(app, store)
        .dns([GoogleDns])
        .dns([CloudflareDns])
        .dns([Quad9Dns])
        .dns([MyDnsSvc])
        .build()?;

    Ok(())
}
