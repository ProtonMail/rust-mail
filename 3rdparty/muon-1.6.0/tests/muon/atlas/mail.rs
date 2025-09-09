use crate::atlas::{new_client, PASS, USER};
use anyhow::{bail, Result};
use muon::client::flow::LoginFlow;
use muon::GET;
use serde_json::Value;

#[tokio::test]
async fn test_mail_message_ids() -> Result<()> {
    let client = match new_client().auth().login(USER, PASS).await {
        LoginFlow::Ok(c, _) => c,
        LoginFlow::TwoFactor(_, _) => bail!("unexpected TFA flow"),
        LoginFlow::Failed { .. } => bail!("unexpected failure"),
    };

    let req = GET!("/mail/v4/messages/ids").query(("Limit", 1000));
    let res = client.send(req).await?;
    let res: Value = res.ok()?.into_body_json()?;

    println!("message ids: {res:#?}");

    Ok(())
}
