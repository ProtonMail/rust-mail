use crate::{
    MailUserContext,
    datatypes::attachment::ContentId,
    models::{AttachmentData, MailSettings},
};
use proton_core_api::{
    service::{ApiServiceError, ApiServiceResult},
    services::proton::ProtonCore,
};
use reqwest::Method;
use stash::stash::StashError;
use std::{str::FromStr, sync::Weak};
use thiserror::Error as TError;
use tracing::instrument;
use url::Url;

#[derive(Debug)]
pub struct ImageLoader {
    ctx: Weak<MailUserContext>,
}

impl ImageLoader {
    pub fn new(ctx: Weak<MailUserContext>) -> Self {
        Self { ctx }
    }

    /// Loads image from `url`, using the Proton image proxy if possible.
    ///
    /// If given address uses the `cid` (content id) schema, this function
    /// fetches data using the `load_cid` callback.
    #[instrument(skip(self, url, load_cid))]
    pub async fn load<L, C>(
        &self,
        url: &str,
        policy: ImagePolicy,
        load_cid: L,
    ) -> Result<AttachmentData, ImageLoaderError<C>>
    where
        L: AsyncFnOnce(&ContentId) -> Result<AttachmentData, C>,
    {
        // If the URL is empty, return an empty attachment that ends up being
        // displayed as a "question mark image" on the device.
        //
        // This is a somewhat hacky way of handling disabled remote content - if
        // user has that setting active, our html transformer will convert:
        //
        //     <img src="https://funny-dogs.io/123.jpg" />
        //
        // ... into:
        //
        //     <img src="" />
        //
        // ... and that empty `src` is what we get as `url` here.
        if url.trim().is_empty() {
            return Ok(AttachmentData::empty());
        }

        let url = Url::from_str(url)?;

        match url.scheme() {
            "cid" => load_cid(&url.path().into())
                .await
                .map_err(ImageLoaderError::LoadCid),

            "http" | "https" => self.fetch(policy, url).await,

            // On iOS you cannot provide a custom handler for the http and https
            // protocols within a webview, so we instead we create a made-up
            // protocol that only our app understands
            "proton-http" | "proton-https" => {
                // We can't use `url.set_scheme()`, because the crate we're
                // using prevents us from switching into protected protocols
                let url = String::from(url).replacen("proton-", "", 1);
                let url = Url::parse(&url)?;

                self.fetch(policy, url).await
            }

            scheme => Err(ImageLoaderError::UnexpectedScheme(scheme.into())),
        }
    }

    #[instrument(skip_all)]
    async fn fetch<C>(
        &self,
        policy: ImagePolicy,
        mut url: Url,
    ) -> Result<AttachmentData, ImageLoaderError<C>> {
        let ctx = self.ctx.upgrade().ok_or(ImageLoaderError::LostContext)?;
        let use_proxy;

        match policy {
            ImagePolicy::Safe => {
                if url.scheme() == "http" {
                    url.set_scheme("https").unwrap();
                }

                let tether = ctx.user_stash().connection().await?;

                use_proxy = MailSettings::get_or_default(&tether)
                    .await
                    .is_proxy_enabled();
            }

            ImagePolicy::Unsafe => {
                use_proxy = false;
            }
        };

        let data = if use_proxy {
            Self::fetch_proxied(&ctx, url)
                .await?
                .ok_or(ImageLoaderError::ProxyFailed)?
        } else {
            Self::fetch_direct(&ctx, url).await?
        };

        Ok(AttachmentData {
            data,
            mime: String::from("image/*"),
        })
    }

    #[instrument(skip_all)]
    async fn fetch_proxied(ctx: &MailUserContext, url: Url) -> ApiServiceResult<Option<Vec<u8>>> {
        let data = ctx.session().proxy_img(&url, false).await?;

        // Yes, proxy returns an empty response if the image failed to be loaded
        if data.image.is_empty() {
            Ok(None)
        } else {
            Ok(Some(data.image))
        }
    }

    #[instrument(skip_all)]
    async fn fetch_direct(ctx: &MailUserContext, url: Url) -> ApiServiceResult<Vec<u8>> {
        // Since we can't easily mock https requests, for testing purposes let's
        // mock them with a query param instead
        #[cfg(any(test, feature = "test-utils"))]
        let url = if url.scheme() == "https" {
            let mut url = url;

            url.set_query(Some("https=1"));
            url.set_scheme("http").unwrap();
            url
        } else {
            url
        };

        let response = ctx
            .http_client()
            .request(Method::GET, url)
            .header("User-Agent", "proton-mail/7.0.0")
            .send()
            .await
            .map_err(|e| ApiServiceError::ConnectionError(e.to_string()))?;

        let response = response.error_for_status().map_err(|e| {
            ApiServiceError::UnknownError(format!("Couldn't fetch image (direct): {e:?}"))
        })?;

        let response = response
            .bytes()
            .await
            .map_err(|e| {
                ApiServiceError::UnknownError(format!("Couldn't fetch image (direct): {e:?}"))
            })?
            .to_vec();

        Ok(response)
    }
}

#[derive(Debug, TError)]
pub enum ImageLoaderError<C> {
    #[error(transparent)]
    Api(#[from] ApiServiceError),

    #[error("Couldn't load inline image: {0}")]
    LoadCid(C),

    #[error("Lost context")]
    LostContext,

    #[error("Couldn't load image via the image proxy (got empty response)")]
    ProxyFailed,

    #[error(transparent)]
    Stash(#[from] StashError),

    #[error("Unexpected image scheme: {0}")]
    UnexpectedScheme(String),

    #[error(transparent)]
    Url(#[from] url::ParseError),
}

#[derive(Clone, Copy, Debug)]
pub enum ImagePolicy {
    /// Swap image's protocol from `http` to `https` and allow the image to be
    /// proxied through Proton severs (assuming user has this option enabled).
    Safe,

    /// Load image as-is, without changing the protocol and without passing it
    /// through the proxy.
    Unsafe,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_context::MailTestContext;
    use proton_core_common::datatypes::ImageProxy;
    use stash::orm::Model;
    use test_case::test_case;
    use wiremock::{
        Mock, ResponseTemplate,
        matchers::{method, path, query_param},
    };

    struct TestCase {
        given_cfg: ImageProxy,
        given_url: &'static str,
        given_policy: ImagePolicy,
        expected_request: ExpectedRequest,
    }

    #[derive(Clone, Copy, Debug)]
    enum ExpectedRequest {
        Direct(&'static str),
        Proxied(&'static str),
    }

    const TEST_HTTP_DIRECT_SAFE: TestCase = TestCase {
        given_cfg: ImageProxy::empty(),
        given_url: "http://le.ona/covers/bleeding-love.tiff",
        given_policy: ImagePolicy::Safe,
        expected_request: ExpectedRequest::Direct("https://le.ona/covers/bleeding-love.tiff"),
    };

    const TEST_HTTP_DIRECT_UNSAFE: TestCase = TestCase {
        given_cfg: ImageProxy::empty(),
        given_url: "http://le.ona/covers/bleeding-love.tiff",
        given_policy: ImagePolicy::Unsafe,
        expected_request: ExpectedRequest::Direct("http://le.ona/covers/bleeding-love.tiff"),
    };

    const TEST_HTTP_PROXIED_SAFE: TestCase = TestCase {
        given_cfg: ImageProxy::all(),
        given_url: "http://le.ona/covers/bleeding-love.tiff",
        given_policy: ImagePolicy::Safe,
        expected_request: ExpectedRequest::Proxied("https://le.ona/covers/bleeding-love.tiff"),
    };

    const TEST_HTTP_PROXIED_UNSAFE: TestCase = TestCase {
        given_cfg: ImageProxy::all(),
        given_url: "http://le.ona/covers/bleeding-love.tiff",
        given_policy: ImagePolicy::Unsafe,
        expected_request: ExpectedRequest::Direct("http://le.ona/covers/bleeding-love.tiff"),
    };

    const TEST_HTTPS_DIRECT_SAFE: TestCase = TestCase {
        given_cfg: ImageProxy::empty(),
        given_url: "https://le.ona/covers/bleeding-love.tiff",
        given_policy: ImagePolicy::Safe,
        expected_request: ExpectedRequest::Direct("https://le.ona/covers/bleeding-love.tiff"),
    };

    const TEST_HTTPS_DIRECT_UNSAFE: TestCase = TestCase {
        given_cfg: ImageProxy::empty(),
        given_url: "https://le.ona/covers/bleeding-love.tiff",
        given_policy: ImagePolicy::Unsafe,
        expected_request: ExpectedRequest::Direct("https://le.ona/covers/bleeding-love.tiff"),
    };

    const TEST_HTTPS_PROXIED_SAFE: TestCase = TestCase {
        given_cfg: ImageProxy::all(),
        given_url: "https://le.ona/covers/bleeding-love.tiff",
        given_policy: ImagePolicy::Safe,
        expected_request: ExpectedRequest::Proxied("https://le.ona/covers/bleeding-love.tiff"),
    };

    const TEST_HTTPS_PROXIED_UNSAFE: TestCase = TestCase {
        given_cfg: ImageProxy::all(),
        given_url: "https://le.ona/covers/bleeding-love.tiff",
        given_policy: ImagePolicy::Unsafe,
        expected_request: ExpectedRequest::Direct("https://le.ona/covers/bleeding-love.tiff"),
    };

    // Make sure that `proton-http` behaves same as `http`
    const TEST_PROTON_HTTP_DIRECT_SAFE: TestCase = TestCase {
        given_url: "proton-http://le.ona/covers/bleeding-love.tiff",
        ..TEST_HTTP_DIRECT_SAFE
    };

    // Make sure that `proton-http` behaves same as `http`
    const TEST_PROTON_HTTP_DIRECT_UNSAFE: TestCase = TestCase {
        given_url: "proton-http://le.ona/covers/bleeding-love.tiff",
        ..TEST_HTTP_DIRECT_UNSAFE
    };

    // Make sure that `proton-http` behaves same as `http`
    const TEST_PROTON_HTTP_PROXIED_SAFE: TestCase = TestCase {
        given_url: "proton-http://le.ona/covers/bleeding-love.tiff",
        ..TEST_HTTP_PROXIED_SAFE
    };

    // Make sure that `proton-http` behaves same as `http`
    const TEST_PROTON_HTTP_PROXIED_UNSAFE: TestCase = TestCase {
        given_url: "proton-http://le.ona/covers/bleeding-love.tiff",
        ..TEST_HTTP_PROXIED_UNSAFE
    };

    // Make sure that `proton-https` behaves same as `https`
    const TEST_PROTON_HTTPS_DIRECT_SAFE: TestCase = TestCase {
        given_url: "proton-https://le.ona/covers/bleeding-love.tiff",
        ..TEST_HTTPS_DIRECT_SAFE
    };

    // Make sure that `proton-https` behaves same as `https`
    const TEST_PROTON_HTTPS_DIRECT_UNSAFE: TestCase = TestCase {
        given_url: "proton-https://le.ona/covers/bleeding-love.tiff",
        ..TEST_HTTPS_DIRECT_UNSAFE
    };

    // Make sure that `proton-https` behaves same as `https`
    const TEST_PROTON_HTTPS_PROXIED_SAFE: TestCase = TestCase {
        given_url: "proton-https://le.ona/covers/bleeding-love.tiff",
        ..TEST_HTTPS_PROXIED_SAFE
    };

    // Make sure that `proton-https` behaves same as `https`
    const TEST_PROTON_HTTPS_PROXIED_UNSAFE: TestCase = TestCase {
        given_url: "proton-https://le.ona/covers/bleeding-love.tiff",
        ..TEST_HTTPS_PROXIED_UNSAFE
    };

    #[test_case(TEST_HTTP_DIRECT_SAFE)]
    #[test_case(TEST_HTTP_DIRECT_UNSAFE)]
    #[test_case(TEST_HTTP_PROXIED_SAFE)]
    #[test_case(TEST_HTTP_PROXIED_UNSAFE)]
    // ---
    #[test_case(TEST_HTTPS_DIRECT_SAFE)]
    #[test_case(TEST_HTTPS_DIRECT_UNSAFE)]
    #[test_case(TEST_HTTPS_PROXIED_SAFE)]
    #[test_case(TEST_HTTPS_PROXIED_UNSAFE)]
    // ---
    #[test_case(TEST_PROTON_HTTP_DIRECT_SAFE)]
    #[test_case(TEST_PROTON_HTTP_DIRECT_UNSAFE)]
    #[test_case(TEST_PROTON_HTTP_PROXIED_SAFE)]
    #[test_case(TEST_PROTON_HTTP_PROXIED_UNSAFE)]
    // ---
    #[test_case(TEST_PROTON_HTTPS_DIRECT_SAFE)]
    #[test_case(TEST_PROTON_HTTPS_DIRECT_UNSAFE)]
    #[test_case(TEST_PROTON_HTTPS_PROXIED_SAFE)]
    #[test_case(TEST_PROTON_HTTPS_PROXIED_UNSAFE)]
    #[tokio::test]
    async fn test(case: TestCase) {
        let ctx = MailTestContext::new().await;
        let uctx = ctx.uninitialized_mail_user_context().await;

        // ---
        // Update mail settings

        let mut tether = uctx.user_stash().connection().await.unwrap();

        tether
            .tx(async |bond| {
                MailSettings {
                    image_proxy: case.given_cfg,
                    ..MailSettings::get_or_default(bond).await
                }
                .save(bond)
                .await
            })
            .await
            .unwrap();

        // ---
        // Prepare image request mock

        match case.expected_request {
            ExpectedRequest::Direct(url) => {
                // For mocking direct requests we replace the fake `le.ona`
                // domain with `localhost`.

                if let Some(url) = url.strip_prefix("http://le.ona") {
                    Mock::given(method("GET"))
                        .and(path(url))
                        .respond_with(ResponseTemplate::new(200).set_body_bytes([1, 2, 3]))
                        .expect(1)
                        .mount(&ctx.mock_web_server)
                        .await;
                } else if let Some(url) = url.strip_prefix("https://le.ona") {
                    // Instead of mocking genuine `https` requests, we rely on
                    // query params -- see `fetch_direct()`

                    Mock::given(method("GET"))
                        .and(path(url))
                        .and(query_param("https", "1"))
                        .respond_with(ResponseTemplate::new(200).set_body_bytes([1, 2, 3]))
                        .expect(1)
                        .mount(&ctx.mock_web_server)
                        .await;
                } else {
                    unreachable!();
                }
            }

            ExpectedRequest::Proxied(url) => {
                ctx.mock_proxy_img(url, vec![1, 2, 3]).await;
            }
        }

        // ---
        // Let's roll

        let url = match case.expected_request {
            ExpectedRequest::Direct(_) => {
                // For mocking direct requests we replace the fake `le.ona`
                // domain with `localhost`.

                case.given_url
                    .replace("le.ona", &ctx.mock_web_server.address().to_string())
            }

            ExpectedRequest::Proxied(_) => case.given_url.to_string(),
        };

        let img = uctx
            .image_loader()
            .load(&url, case.given_policy, async |_| Err(()))
            .await
            .unwrap();

        assert_eq!(vec![1, 2, 3], img.data);
        assert_eq!("image/*", img.mime);
    }

    #[tokio::test]
    async fn load_cid() {
        let ctx = MailTestContext::new().await;
        let uctx = ctx.uninitialized_mail_user_context().await;

        // ---

        let url = "cid:raise-your-horns";
        let policy = ImagePolicy::Safe;

        let img = uctx
            .image_loader()
            .load::<_, ()>(url, policy, async |cid| {
                assert_eq!("raise-your-horns", cid.as_str());

                Ok(AttachmentData {
                    data: vec![1, 2, 3],
                    mime: "image/bmp".into(),
                })
            })
            .await
            .unwrap();

        assert_eq!(vec![1, 2, 3], img.data);
        assert_eq!("image/bmp", img.mime);
    }

    #[tokio::test]
    async fn load_empty() {
        let ctx = MailTestContext::new().await;
        let uctx = ctx.uninitialized_mail_user_context().await;

        // ---

        let url = "";
        let policy = ImagePolicy::Safe;

        let img = uctx
            .image_loader()
            .load(url, policy, async |_| Err(()))
            .await
            .unwrap();

        assert_eq!(AttachmentData::empty(), img);
    }

    #[tokio::test]
    async fn load_proxied_err() {
        let ctx = MailTestContext::new().await;
        let uctx = ctx.uninitialized_mail_user_context().await;

        // ---

        ctx.mock_proxy_img("https://le.ona/covers/bleeding-love.tiff", vec![])
            .await;

        let url = "https://le.ona/covers/bleeding-love.tiff";
        let policy = ImagePolicy::Safe;

        let err = uctx
            .image_loader()
            .load(url, policy, async |_| Err(()))
            .await
            .unwrap_err();

        assert!(matches!(err, ImageLoaderError::ProxyFailed));
    }
}
