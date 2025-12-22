use super::test_context::MailTestContext;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path, query_param},
};

impl MailTestContext {
    #[function_name::named]
    pub async fn mock_proxy_img(&self, url: &str, img: Vec<u8>, content_type: &str) {
        Mock::given(method("GET"))
            .and(path("/api/core/v4/images"))
            .and(query_param("Url", url))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(img)
                    .insert_header("Content-Type", content_type),
            )
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }
}
