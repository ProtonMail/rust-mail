use anyhow::Result;
use muon::test::store::TestStore;
use muon::util::ProtonRequestExt;
use muon::{App, Client, GET};
use wasm_bindgen_test::*;

#[wasm_bindgen_test]
#[cfg_attr(ci, ignore = "proxy required in CI")]
async fn test_ping() -> Result<()> {
    let app = App::default();
    let store = TestStore::atlas();
    let client = Client::builder(app, store).build()?;

    GET!("/tests/ping").send_with(&client).await?.ok()?;

    Ok(())
}
