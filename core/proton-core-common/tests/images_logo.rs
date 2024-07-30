use common::TestContext;
use proton_api_core::services::proton::requests::GetImagesLogoOptions;
use proton_core_common::images_logo::Key;

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

    let image = user_ctx
        .image_for_sender(TEST_ADDRESS, None, None, None, None)
        .await
        .expect("failed to get image")
        .expect("should have value");

    assert_eq!(
        image.to_vec(),
        vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07]
    );
    assert_eq!(user_ctx.images_logo_cache.len(), 1);

    user_ctx
        .image_for_sender(TEST_ADDRESS, None, None, None, None)
        .await
        .expect("failed to get image")
        .expect("should have value");

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
    let key = create_test_key(TEST_ADDRESS);
    user_ctx.images_logo_cache.add_item(key, b"abcdef").unwrap();

    // Get image
    let image = user_ctx
        .image_for_sender(TEST_ADDRESS, None, None, None, None)
        .await
        .expect("failed to get image")
        .expect("should have value");

    // Image is the one from cache
    assert_eq!(std::str::from_utf8(&image.to_vec()).unwrap(), "abcdef");
}

fn create_test_key(address: &str) -> GetImagesLogoOptions {
    let mut options = GetImagesLogoOptions::default();
    options.address = Some(address.to_owned());
    options
}
