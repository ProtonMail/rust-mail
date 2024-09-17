use common::TestContext;
use futures::future::join_all;
use proton_core_common::models::sender_image_cache::SenderImage;
use stash::orm::Model;
use std::fs;

mod common;

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
        .expect("failed to get image");

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
async fn get_sender_image_from_cache() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    ctx.catch_all().await;

    // No user image in cache
    assert!(user_ctx.images_logo_cache.is_empty());

    // Add an item into cache
    let mut key = create_test_key(TEST_ADDRESS);
    key.save_using(user_ctx.stash()).await.unwrap();
    user_ctx.images_logo_cache.add_item(key, b"abcdef").unwrap();

    // Get image
    let image_path = user_ctx
        .image_for_sender(TEST_ADDRESS, None, None, None, None, user_ctx.stash())
        .await
        .expect("failed to get image");

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
        ))
    }

    let result = join_all(requests).await;
    for result in result {
        let image_path = result.unwrap();
        assert_eq!(
            fs::read(image_path).unwrap(),
            vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07]
        );
    }

    let key = SenderImage {
        local_id: None,
        address: Some(TEST_ADDRESS.to_string()),
        bimi_selector: None,
        domain: None,
        format: None,
        max_scale_up_factor: None,
        mode: None,
        size: None,
        row_id: None,
        stash: None,
    };

    let (query, params) = key.build_query();
    let items = SenderImage::find(query, params, user_ctx.stash(), None)
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
}

fn create_test_key(address: &str) -> SenderImage {
    let mut key = SenderImage::default();
    key.address = Some(address.to_owned());
    key
}
