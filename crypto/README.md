
# Proton Cryptography Rust

## Account Crypto

The `proton-crypto-account` crate provides key models (e.g., User Keys, Address keys, etc.) and key management operations building on `proton-crypto`. Note that the crate re-exports the underlying `proton-crypto` dependency.

## Inbox Crypto

The `proton-crypto-inbox` crate provides crypto models and operations related to the Proton inbox (e.g., email encryption/decryption, attachment encryption/decryption). The crate builds on `proton-crypto`/`proton-crypto-account` and re-exports them.

The `proton-crypto-inbox-mime` crate provides utility around parsing and creating MIME encoded inbox messages within Proton.