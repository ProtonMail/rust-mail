# Proton Cryptography Rust

This workspace provides several utility crates for cryptographic operations within Proton.

## Core Proton Crypto

The `proton-crypto` crate contains the core library for crypto operations. In particular:

- It defines a generic API for OpenPGP operations, which is the core cryptographic protocol in most products. An implementation of the API can be accessed through `new_pgp_provider` providing a default `PGPProvider`.
- An implementation of the `PGPProvider` using GopenPGP via the `gopenpgp-sys` wrapper.
- SRP API for authentication and an implementation using GopenPGP via the `gopenpgp-sys` wrapper. The default SRP provider can be accessed via `new_srp_provider`.

## Account Crypto

The `proton-crypto-account` crate provides key models (e.g., User Keys, Address keys, etc.) and key management operations building on `proton-crypto`. Note that the crate re-exports the underlying `proton-crypto` dependency.

## Inbox Crypto

The `proton-crypto-inbox` crate provides crypto models and operations related to the Proton inbox (e.g., email encryption/decryption, attachment encryption/decryption). The crate builds on `proton-crypto`/`proton-crypto-account` and re-exports them.

The `proton-crypto-inbox-mime` crate provides utility around parsing and creating MIME encoded inbox messages within Proton.