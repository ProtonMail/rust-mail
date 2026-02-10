# wiremock-multipart

This project provides matchers dealing with multipart requests for the
awesome [wiremock](https://crates.io/crates/wiremock) testing framework.

# How to install
Add `wiremock-multipart` to your development dependencies:
```toml
[dev-dependencies]
# ...
wiremock-multipart = "0.1"
```
If you are using [`cargo-edit`](https://github.com/killercup/cargo-edit), run
```bash
cargo add wiremock-multipart --dev
```

## Getting started
```rust
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::method;
use wiremock_multipart::prelude::*;

#[async_std::main]
async fn main() {
    // Start a background HTTP server on a random local port
    let mock_server = MockServer::start().await;

    // Arrange the behaviour of the MockServer adding a Mock
    Mock::given(method("POST"))
        .and(NumberOfParts(2))
        .respond_with(ResponseTemplate::new(200))
        // Mounting the mock on the mock server - it's now effective!
        .mount(&mock_server)
        .await;

    // if we now send a multipart/form-data request with two parts to it, the request
    // will match and return 200.
}
```
