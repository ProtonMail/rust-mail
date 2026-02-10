use anyhow::Result;
use muon::app::AppVersion;
use muon::common::prelude::*;
use muon::common::{Host, Server};
use muon::dns::{DohClient, DohService, GoogleDoh, Quad9Doh};
use muon::env::{DynEnv, Env, Prod};
use muon::http::HyperConnector;
use muon::rt::{AsyncDialer, AsyncResolver, AsyncSpawner, DynResolver, Resolver};
use muon::test::store::TestStore;
use muon::tls::{BaseTrustAnchor, BaseVerifier, RustlsTls, TlsExt, TlsPin, TlsPinSet};
use muon::util::*;
use muon::{autoimpl, App, Client, GET};

#[tokio::test]
async fn test_doh_normal_routing() -> Result<()> {
    // Create a new app.
    let app = App::new("linux-mail@4.1.0")?;
    let store = TestStore::prod();

    // Create the client with the standard environment.
    let client = Client::builder(app, store)
        .doh([GoogleDoh])
        .doh([Quad9Doh])
        .build()?;

    // Ping should work; we'll use standard routing.
    match client.send(GET!("/tests/ping")).await {
        Ok(res) => assert_eq!(res.server().name(), res.name()),
        Err(err) => panic!("unexpected error: {err}"),
    };

    Ok(())
}

#[tokio::test]
async fn test_doh_alt_routing() -> Result<()> {
    // Create a new app.
    let app = App::new("linux-mail@4.1.0")?;
    let store = TestStore::custom(Prod::default().indirect_only());

    // Create the client with the custom environment.
    let client = Client::builder(app, store)
        .doh([GoogleDoh])
        .doh([Quad9Doh])
        .build()?;

    // Ping should work; we'll use alternate routing.
    match client.send(GET!("/tests/ping").allowed_time(30.s())).await {
        Ok(res) => assert_ne!(res.server().name(), res.name()),
        Err(err) => panic!("unexpected error: {err}"),
    };

    Ok(())
}

#[tokio::test]
async fn test_doh_alt_routing_pinning() -> Result<()> {
    let app = App::new("linux-mail@4.1.0")?;
    let store = TestStore::custom(Prod::default().random_pins());

    // Create the client with the custom environment.
    let client = Client::builder(app, store).doh([GoogleDoh]).build()?;

    // Ping should fail; we'll use alternate routing, but the pins are wrong.
    if let Ok(res) = client.send(GET!("/tests/ping")).await {
        panic!("unexpected success: {res}");
    }

    Ok(())
}

#[tokio::test]
async fn test_doh_client_direct() -> Result<()> {
    for sub in ["mail-api", "verify-api"] {
        let host = Host::direct(format!("{sub}.proton.me"))?;
        let want = AsyncResolver.resolve(&host).await?.into_set();

        let have = new_resolver(GoogleDoh)?.resolve(&host).await?;
        assert_eq!(have.into_set(), want);

        let have = new_resolver(Quad9Doh)?.resolve(&host).await?;
        assert_eq!(have.into_set(), want);
    }

    Ok(())
}

#[tokio::test]
async fn test_doh_client_indirect() -> Result<()> {
    for sub in ["mail-api", "verify-api"] {
        let dir = format!("{sub}.proton.me").as_b32();
        let alt = Host::indirect(format!("d{dir}.protonpro.xyz"))?;

        let have = new_resolver(GoogleDoh)?.resolve(&alt).await?;
        assert!(!have.into_set().is_empty());

        let have = new_resolver(Quad9Doh)?.resolve(&alt).await?;
        assert!(!have.into_set().is_empty());
    }

    Ok(())
}

fn new_resolver(service: impl DohService) -> Result<DynResolver> {
    let connector = HyperConnector::new(
        AsyncSpawner::default(),
        AsyncResolver,
        AsyncDialer,
        RustlsTls.build_any(BaseTrustAnchor, BaseVerifier)?,
        EnvProxy::all("http_proxy"),
    );

    Ok(DohClient::new(service, connector.into_dyn()).into_dyn())
}

#[autoimpl]
trait EnvExt: Env + Sized {
    fn indirect_only(self) -> DynEnv {
        struct EnvImpl<E>(E);

        impl<E: Env> Env for EnvImpl<E> {
            fn servers(&self, version: &AppVersion) -> Vec<Server> {
                (self.0.servers(version))
                    .into_iter()
                    .filter(|s| s.host().is_indirect())
                    .collect()
            }

            fn pins(&self, host: &Host) -> Option<&TlsPinSet> {
                self.0.pins(host)
            }
        }

        EnvImpl(self).into_dyn()
    }

    fn random_pins(self) -> DynEnv {
        struct EnvImpl<E>(E, TlsPinSet);

        impl<E: Env> Env for EnvImpl<E> {
            fn servers(&self, version: &AppVersion) -> Vec<Server> {
                self.0.servers(version)
            }

            fn pins(&self, _: &Host) -> Option<&TlsPinSet> {
                Some(&self.1)
            }
        }

        EnvImpl(self, TlsPinSet::new([TlsPin::new(rand::random())])).into_dyn()
    }
}
