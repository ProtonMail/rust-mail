# Muon

[![pipeline status](https://gitlab.protontech.ch/ProtonVPN/rust/muon/badges/master/pipeline.svg)](https://gitlab.protontech.ch/ProtonVPN/rust/muon/-/pipelines)
[![coverage report](https://gitlab.protontech.ch/ProtonVPN/rust/muon/badges/master/coverage.svg)](https://protonvpn.gitlab-pages.protontech.ch/rust/muon/coverage/index.html)
[![Latest Release](https://gitlab.protontech.ch/ProtonVPN/rust/muon/-/badges/release.svg)](https://gitlab.protontech.ch/ProtonVPN/rust/muon/-/releases)
[![Generated Doc](https://img.shields.io/badge/Doc-Generated-blue)](https://protonvpn.gitlab-pages.protontech.ch/rust/muon/doc/muon)

Muon (named like the particle) is a client library for the Proton API.

## Usage

The `muon` crate is published to the internal Proton registry.

Configure cargo to use the Proton registry by adding the following to your `.cargo/config.toml`:

```toml
[registries.proton]
    index = "sparse+https://rust.gitlab-pages.protontech.ch/shared/registry/index/"
```

With the registry configured, you can add `muon` to your project with:

```sh
$ cargo add muon --registry proton
```

See the relevant Cargo [documentation] for more information.

[documentation]: https://doc.rust-lang.org/cargo/reference/registries.html

## Examples

You can find a variety of examples in the [examples](./examples) directory.
Each is a standalone crate that demonstrates a specific feature of the library.

### Authentication Examples

- **[auth-login](./examples/auth-login.rs)**: Basic login example.
- **[auth-logout](./examples/auth-logout.rs)**: Basic logout example.
- **[auth-external](./examples/auth-external.rs)**: Demonstrates using an external session (e.g., from a browser)
- **[auth-fork](./examples/auth-fork.rs)**: Shows how to fork a session.
- **[auth-fork-user-code](./examples/auth-fork-user-code.rs)**: Example of forking a session via a user code.

### Builder Examples

- **[builder-simple](./examples/builder-simple.rs)**: Shows simple client builder usage.
- **[builder-dns](./examples/builder-dns.rs)**: Shows how to add DNS services to a client.
- **[builder-doh](./examples/builder-doh.rs)**: Shows how to add DNS-over-HTTPS services to a client.

### Client Examples

- **[client-requests](./examples/client-requests.rs)**: Basic example of sending requests.
- **[client-responses](./examples/client-responses.rs)**: Demonstrates how to handle responses.
- **[client-timeout-policy](./examples/client-timeout-policy.rs)**: Shows timeout policy usage.
- **[client-retry-policy](./examples/client-retry-policy.rs)**: Shows retry policy usage.
- **[client-middleware](./examples/client-middleware.rs)**: Shows how to add pre-defined and custom middleware to a client.

### Custom Components

- **[custom-env](./examples/custom-env.rs)**: Shows how to define a custom environment.
- **[custom-tls](./examples/custom-tls.rs)**: Shows how to define custom trust anchors.

### Testing Examples

- **[muon-test-server](./examples/muon-test-server/src/main.rs)**: Demonstrates the local server implemented in `muon-test`.

### Demos

- **[demo-mail](./examples/demo-mail/src/main.rs)**: A simple app that dumps a user's mailbox to disk.

## Development

### Tests

The primary test suite is located in the `muon` crate's `tests` directory.
This directory contains two integration test suites, [muon] and [muon-wasm].

[muon]: ./muon/tests/muon
[muon-wasm]: ./muon/tests/muon-wasm

The `muon` test suite is intended to be built for the host's native architecture,
while the `muon-wasm` test suite is intended to be built for WebAssembly.

The tests are feature-gated; different tests will run depending on the features enabled.
The available features are:

- `test-atlas`: Enables tests against the Atlas environment (requires dev VPN),
- `test-local`: Enables tests against a local API server,
- `test-proxy`: Enables tests that require a proxy server (requires `tinyproxy`),
- `test-dns`: Enables DNS tests,
- `test-doh`: Enables DNS-over-HTTPS tests.

To simply run all tests, use:

```sh
$ cargo test --all-features
```

To run a specific kind of test, use something like:

```sh
$ cargo test -F test-dns -F test-doh
```

To run the WebAssembly tests, ensure Node.js and `wasm-bindgen-cli` are installed, then run:

```sh
$ # pacman -S nodejs (or equivalent)
$ # cargo install wasm-bindgen-cli
$ cargo test --all-features --target wasm32-unknown-unknown
```

### Release Process

The release process is semi-automated using GitLab CI.

When a merge request targeting the `master` branch is opened, the CI pipeline will run the various linters, build the crates, and run the tests.
If successful, the merge request will be allowed to merge.

Once merged to `master`, the CI pipeline will run again, but this time also doing the following:

- Generating the documentation and publishing it to GitLab Pages,
- Triggering a set of downstream jobs to publish each crate to the Proton registry.

The downstream jobs appear in the GitLab UI as a "manual" job; they must be manually started by clicking the "play" button.
Each job, when manually started, will build and package its respective crate and publish it to the
[Proton registry] by opening a merge request. The merge request must be approved and merged to publish the crate.

[Proton registry]: https://gitlab.protontech.ch/rust/shared/registry

### Pre-Commit Hook

A pre-commit hook is provided that runs a few linters and local tests before committing.
This is a subset of the checks that the CI pipeline runs.
The hook can be enabled like so:

```sh
$ ln -s $(pwd)/.hooks/pre-commit .git/hooks/pre-commit
```

## Contributing

To contribute to `muon`, please follow the [contributing guide](CONTRIBUTING.md). 

## Features, Requirements, and Roadmap

### MVP requirements

- [x] Async code
- [x] Session handling, including Access Token refreshing.
- [x] Clean + strict definition of environments (=> impossible to "cast" an environment in another, impossible to use prod without pinning, etc.)
- [x] Access token storage / retrieval
- [x] Plug-able transport (so that VPN can plug alternative routing feature, guest hole, etc)
- [x] Clean definition of App profile (i.e. app-version, user-agent, client-secret, etc)
- [x] Sending API request.

### Additional requirements

- [x] Rust doc
- [x] Authentication (Proton SRP)
- [x] No rust-only patterns
- [x] Efficient TLS/HTTP/HTTP2 behavior (i.e. proper connection pooling, keep-alive, TLS session reuse, etc.)
- [ ] Port at least the implemented part of the test suite of python-proton-core and python-proton-core-internal.

### Product scope

- [x] Only session logic, no product logic at all. This is strict.

### Roadmap

- Auth
  - [x] Login
  - [x] Logout
  - [x] 2FA
  - [x] Session forking
  - [ ] Human verification
- Requests and Responses
  - [x] Timing requirements
  - [x] Urgency requirements
  - [x] Handling of 429 + 50x errors
- Anti-Censorship
  - [x] Alternative Routing (AR)
  - [ ] Guest holes (for testing the plug-able transports, explicit AR might be an option)
- [ ] Proof of Work (PoW)
- [ ] FFI interfaces
