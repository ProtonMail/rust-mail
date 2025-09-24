use anyhow::Result;
use muon::deps::itertools::Itertools;
use muon::test::server::Server;
use muon::util::{IntoIterExt, ProtonRequestExt};
use muon::{App, GET};
use std::io::Cursor;
use std::sync::Arc;

#[muon::test]
async fn test_header_app_version(s: Arc<Server>) -> Result<()> {
    let r = s.new_recorder();

    // Two different app versions.
    let foo_app = App::new("windows-mail@1.2.3")?;
    let bar_app = App::new("linux-mail@4.5.6")?;

    // Send a request with the `foo` app version.
    s.client_for(foo_app).send(GET!("/tests/ping")).await?;
    let req = r.read().pop_back().unwrap();
    let hdr = req.headers().get("x-pm-appversion").unwrap();
    assert_eq!(hdr.to_str()?, "windows-mail@1.2.3");

    // Send a request with the `bar` app version.
    s.client_for(bar_app).send(GET!("/tests/ping")).await?;
    let req = r.read().pop_back().unwrap();
    let hdr = req.headers().get("x-pm-appversion").unwrap();
    assert_eq!(hdr.to_str()?, "linux-mail@4.5.6");

    Ok(())
}

#[muon::test]
async fn test_header_user_agent(s: Arc<Server>) -> Result<()> {
    let r = s.new_recorder();

    // Two different user agents.
    let foo_app = App::default().with_user_agent("foo");
    let bar_app = App::default().with_user_agent("bar");

    // Send a request with the `foo` user agent.
    s.client_for(foo_app).send(GET!("/tests/ping")).await?;
    let req = r.read().pop_back().unwrap();
    let hdr = req.headers().get("user-agent").unwrap();
    assert_eq!(hdr.to_str()?, "foo");

    // Send a request with the `bar` user agent.
    s.client_for(bar_app).send(GET!("/tests/ping")).await?;
    let req = r.read().pop_back().unwrap();
    let hdr = req.headers().get("user-agent").unwrap();
    assert_eq!(hdr.to_str()?, "bar");

    Ok(())
}

#[muon::test]
async fn test_header_duplicates(s: Arc<Server>) -> Result<()> {
    let r = s.new_recorder();
    let c = s.client();

    GET!("/tests/ping")
        .header(("foo", "bar"))
        .header(("foo", "baz"))
        .header(("foo", "qux"))
        .send_with(&c)
        .await?
        .ok()?;

    let req = r.read().pop_back().unwrap();
    let hdr = req.headers().get_all("foo").into_vec();
    assert_eq!(hdr.len(), 3);
    assert_eq!(hdr[0].to_str()?, "bar");
    assert_eq!(hdr[1].to_str()?, "baz");
    assert_eq!(hdr[2].to_str()?, "qux");

    Ok(())
}

#[muon::test]
async fn test_query(s: Arc<Server>) -> Result<()> {
    let r = s.new_recorder();
    let c = s.client();

    GET!("/tests/ping")
        .query(("bar", "baz"))
        .query(("qux",))
        .query((1, 2))
        .send_with(&c)
        .await?
        .ok()?;

    let req = r.read().pop_back().unwrap();
    let qry = req.uri().query().unwrap();
    assert_eq!(qry, "bar=baz&qux&1=2");

    GET!("/tests/ping")
        .query(("🦄", "🌈"))
        .send_with(&c)
        .await?
        .ok()?;

    let req = r.read().pop_back().unwrap();
    let qry = req.uri().query().unwrap();
    assert_eq!(qry, "%F0%9F%A6%84=%F0%9F%8C%88");

    Ok(())
}

#[muon::test]
async fn test_multipart_request(s: Arc<Server>) -> Result<()> {
    use common_multipart_rfc7578::client::multipart::{self, BoundaryGenerator};

    struct TestGenerator;

    impl BoundaryGenerator for TestGenerator {
        fn generate_boundary() -> String {
            "test_boundry".to_string()
        }
    }

    let r = s.new_recorder();
    let c = s.client();
    let pic = "Not quite a picture but will do".to_string();

    GET!("/tests/ping")
        .multipart(move |_default_body| {
            let mut body = multipart::Form::new::<TestGenerator>();
            body.add_text("MyText", "1234");
            body.add_reader_file("ProfilePic", Cursor::new(pic), "pic.jpeg");
            body.add_reader_file_with_mime(
                "WhalesSounds",
                Cursor::new("•၊၊||၊|။||||။၊|။•"),
                "WhalesSounds.wav",
                "audio/vnd.wav".parse().unwrap(),
            );
            body
        })
        .await?
        .send_with(&c)
        .await?
        .ok()?;

    let req = r.read().pop_back().unwrap();
    let hdr = req.headers().get_all("Content-type").into_vec();
    let bdy = std::str::from_utf8(req.body()).unwrap();

    assert_eq!(hdr.len(), 1);
    assert_eq!(
        hdr[0].to_str()?,
        "multipart/form-data; boundary=test_boundry"
    );
    assert_eq!(
        bdy,
        r#" --test_boundry
            content-type: text/plain
            content-disposition: form-data; name="MyText"

            1234
            --test_boundry
            content-type: application/octet-stream
            content-disposition: form-data; name="ProfilePic"; filename="pic.jpeg"

            Not quite a picture but will do
            --test_boundry
            content-type: audio/vnd.wav
            content-disposition: form-data; name="WhalesSounds"; filename="WhalesSounds.wav"

            •၊၊||၊|။||||။၊|။•
            --test_boundry--
        "#
        .lines()
        .map(str::trim)
        .join("\r\n")
    );

    Ok(())
}
