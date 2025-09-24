# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.7.1] - 2025-08-27
### Changed
- Replaced muon test server PGP by proton-srp (remove dependecy to gopenpgp-sys)

## [1.7.0] - 2025-08-18
### Added
- Added Fido2 support to the login flow
- Authenticator as a product

## [1.6.0] - 2025-07-09

### Fixed
- Only check the head of a tls cert chain when performing tls pinning checks

### Changed
- Bump rustls-platform-verifier to 0.6
- Bump jni to 0.21
- Deprecated `muon::tls::java_init`

## [1.5.0] - 2025-07-02

### Added
- Added `InfoProvider`. This is used by the muon client to query its clients for the fingerprint (for now) and other information (in the future). See `examples/auth-info-provider.rs` for how to use it.

### Fixed

### Changed
- The fingerprint is set as the body of the request that creates the unauthenticated session (`POST /auth/v4/sessions`).
- When creating a new `LoginFlow` the fingerprint passed using the deprecated API is used. If there's no fingerprint passed with the deprecated API the fingerprint fetched from the new API is used.
- Deprecated:
    - `LoginExtraInfo` and `LoginExtraInfoBuilder`. Use `InfoProvider` to pass the fingerprint to muon.
    - `LoginFlow.new_with_extra`, because it used `LoginExtraInfo`. Use `LoginFlow.new` instead.
    - `AuthFlow.login_with_extra`, because it used `LoginExtraInfo`. Use `AuthFlow.login` instead.

## [1.4.0] - 2025-06-10

### Added
- Added unauthenticated session support. Requests use an unauthenticated session unless there is an authenticated session, or the session is externally managed.
- Errors can be marked as retryable; this is exposed by `Error::retryable`
- Public getters for most `HttpReq` fields

### Fixed
- Moved default feature flags of the `muon-impl` create to here, so that the behaviour of re-exported feature flags match the intent
- Changed `HttpRes::ok()` to support non-standard status codes.

### Changed
- Deprecated `ErrorKind::Closed` in favour of `ErrorKind::Send` + `retryable`
- Requests that fail with _any_ retryable error are retried (previously, only `ErrorKind::Closed` were retried)

## [1.3.0] - 2025-03-28

### Changed
- Muon is now a single crate

### Removed

## [1.2.0] - 2025-01-14

### Changed
- Bumped `muon-impl` to 0.13.0

## [1.1.0] - 2024-12-16

### Changed
- Bumped `muon-impl` to 0.12.0

## [1.0.0] - 2024-12-12

### Added
- Support for serde-qs
- New body conversion methods (body_str, into_body_str)
- Implement PATCH method
- Implement type-safe API auth session scopes
- Implement resume login flow from 2FA stage 

### Changed
- StoreFailure becomes StoreError
- Store trait becomes async
- LoginFlow also returns LoginFlowData

## [0.12.0] - 2024-10-30

### Added
- other-platform feature flag (custom platform)
- other-product feature flag (custom product)

## [0.11.0] - 2024-09-20
### Added
- Request service type
### Changed
- Clients can be created without persistent storage

### Fixed 
- Default timeout (TCP/UDP/TLS) to sane const values

### Removed
- Request timeout setters

## [0.10.0] - 2024-08-24

Refactored API initial version 

### Added
- Documentation and doctest

### Changed
- Change the import structure
- Muon errors do not contain storage error
