use std::{fs, path::PathBuf};

use futures::future::try_join_all;
use pretty_assertions::assert_eq;
use proton_core_common::test_utils::test_context::TestContext;
use test_case::test_case;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

fn test_address() -> String {
    "test@example.com".to_owned()
}

#[tokio::test]
async fn get_empty_sender_image() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    ctx.mock_get_images_logo(vec![]).await;

    let image_path = user_ctx
        .image_for_sender(
            test_address().into(),
            None,
            None,
            None,
            None,
            &mut user_ctx.stash().connection().await.unwrap(),
        )
        .await
        .expect("failed to get image");

    assert!(image_path.is_none());
}

#[tokio::test]
async fn concurrency() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/images/logo"))
        .respond_with(
            ResponseTemplate::new(200).set_body_bytes("This is a very nice image lol".as_bytes()),
        )
        .named("Get images/logo (but allow > 1)")
        .mount(ctx.mock_server())
        .await;

    let requests = (0..30).map(|_| {
        let ctx_clone = user_ctx.clone();
        async move {
            let mut tether = ctx_clone.stash().connection().await.unwrap();
            ctx_clone
                .image_for_sender(test_address().into(), None, None, None, None, &mut tether)
                .await
        }
    });

    let result = try_join_all(requests).await.unwrap();
    for result in result {
        result.unwrap();
    }

    let mut tether = user_ctx.stash().connection().await.unwrap();
    let path_given = user_ctx
        .image_for_sender(test_address().into(), None, None, None, None, &mut tether)
        .await
        .unwrap()
        .unwrap();

    let path = ctx
        .tmp_dir
        .path()
        .join("core-cache")
        .join(ctx.user_context().await.user_id().to_string())
        .join("sender_images")
        .join(format!("{}-0xeb804ef98475036c.svg", test_address(),));
    assert_eq!(path, PathBuf::from(path_given));

    let image = fs::read_to_string(path).unwrap();
    assert_eq!(image, "This is a very nice image lol");
}

#[test_case(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A], "png"; "png")]
#[test_case(&[0x52, 0x49, 0x46, 0x46, 0x00, 0x01, 0x02, 0x03, 0x57, 0x45, 0x42, 0x50], "webp"; "webp")]
#[tokio::test]
async fn image_extension(bytes: &[u8], expected: &str) {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    ctx.mock_get_images_logo(bytes.to_vec()).await;

    let image_path = user_ctx
        .image_for_sender(
            test_address().into(),
            None,
            None,
            None,
            None,
            &mut user_ctx.stash().connection().await.unwrap(),
        )
        .await
        .expect("failed to get image")
        .unwrap();

    assert!(image_path.ends_with(expected));
    assert_eq!(fs::read(&image_path).unwrap(), bytes);

    // Called two times but only calls the api once.
    let image_path_2 = user_ctx
        .image_for_sender(
            test_address().into(),
            None,
            None,
            None,
            None,
            &mut user_ctx.stash().connection().await.unwrap(),
        )
        .await
        .expect("failed to get image")
        .unwrap();
    assert_eq!(image_path, image_path_2);
}
