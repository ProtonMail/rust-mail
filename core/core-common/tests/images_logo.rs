use futures::future::join_all;
use proton_core_common::models::sender_image_cache::{
    ReceivedFormat, SenderImage, SenderImageMetadata,
};
use proton_core_test_utils::test_context::TestContext;
use stash::orm::Model;
use std::fs;
use test_case::test_case;

const TEST_ADDRESS: &str = "test@example.com";

#[tokio::test]
async fn get_sender_image() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    ctx.mock_get_images_logo(vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07])
        .await;
    ctx.catch_all().await;

    // No user image in cache
    assert!(user_ctx.images_logo_cache.is_empty());

    let image_path = user_ctx
        .image_for_sender(TEST_ADDRESS, None, None, None, None, user_ctx.stash())
        .await
        .expect("failed to get image")
        .unwrap();

    assert_eq!(
        fs::read(image_path).unwrap(),
        vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07]
    );
    assert_eq!(user_ctx.images_logo_cache.len(), 1);

    user_ctx
        .image_for_sender(TEST_ADDRESS, None, None, None, None, user_ctx.stash())
        .await
        .expect("failed to get image");

    assert_eq!(user_ctx.images_logo_cache.len(), 1);
}

#[tokio::test]
async fn get_empty_sender_image() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    ctx.mock_get_images_logo(vec![]).await;
    ctx.catch_all().await;

    let image_path = user_ctx
        .image_for_sender(TEST_ADDRESS, None, None, None, None, user_ctx.stash())
        .await
        .expect("failed to get image");

    assert!(image_path.is_none());
}

#[tokio::test]
async fn get_sender_image_from_cache() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    ctx.catch_all().await;

    // No user image in cache
    assert!(user_ctx.images_logo_cache.is_empty());

    // Add an item into cache
    let mut key = create_test_key(TEST_ADDRESS, Some(ReceivedFormat::Png));
    let mut tether = user_ctx.stash().connection();
    let tx = tether.transaction().await.unwrap();
    key.save(&tx).await.unwrap();
    tx.commit().await.unwrap();
    let extra_metadata = SenderImageMetadata {
        received_format: ReceivedFormat::Png,
        is_empty: false,
    };
    user_ctx
        .images_logo_cache
        .add_item_with_extra(key, b"abcdef", &extra_metadata)
        .unwrap();

    // Get image
    let image_path = user_ctx
        .image_for_sender(TEST_ADDRESS, None, None, None, None, user_ctx.stash())
        .await
        .expect("failed to get image")
        .unwrap();

    // Image is the one from cache
    assert_eq!(fs::read(image_path).unwrap(), b"abcdef");
}

#[tokio::test]
async fn concurrent_request() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    ctx.mock_get_images_logo(vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07])
        .await;

    ctx.catch_all().await;

    // No user image in cache
    assert!(user_ctx.images_logo_cache.is_empty());

    let mut requests = vec![];
    for _ in 0..3 {
        requests.push(user_ctx.image_for_sender(
            TEST_ADDRESS,
            None,
            None,
            None,
            None,
            user_ctx.stash(),
        ));
    }

    let result = join_all(requests).await;
    for result in result {
        let image_path = result.unwrap().unwrap();
        assert_eq!(
            fs::read(image_path).unwrap(),
            vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07]
        );
    }

    let key = SenderImage {
        address: Some(TEST_ADDRESS.to_string()),
        ..Default::default()
    };

    let (query, params) = key.build_query();
    let tether = user_ctx.stash().connection();
    let items = SenderImage::find(query, params, &tether).await.unwrap();
    assert_eq!(items.len(), 1);
}

#[test_case(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A], "png"; "png")]
#[test_case(&[0x52, 0x49, 0x46, 0x46, 0x00, 0x01, 0x02, 0x03, 0x57, 0x45, 0x42, 0x50], "webp"; "webp")]
#[tokio::test]
async fn image_extension(bytes: &[u8], expected: &str) {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    ctx.mock_get_images_logo(bytes.to_vec()).await;
    ctx.catch_all().await;

    // No user image in cache
    assert!(user_ctx.images_logo_cache.is_empty());

    let image_path = user_ctx
        .image_for_sender(TEST_ADDRESS, None, None, None, None, user_ctx.stash())
        .await
        .expect("failed to get image")
        .unwrap();

    assert_eq!(image_path.extension().unwrap(), expected);
    let tether = user_ctx.stash().connection();
    let sender_image = SenderImage::load(1.into(), &tether).await.unwrap().unwrap();
    let received_format = sender_image.received_format.unwrap();
    assert_eq!(format!("{received_format}"), expected);
}

#[allow(clippy::field_reassign_with_default)]
fn create_test_key(address: &str, received_format: Option<ReceivedFormat>) -> SenderImage {
    let mut key = SenderImage::default();
    key.address = Some(address.to_owned());
    key.received_format = received_format;
    key
}
