# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - 2024-00-00

### Changed

- `ContactCardType` must derive `Serialize_repr` and `Deserialize_repr`.

## [0.7.2] - 2024-11-22

- Update `proton-crypto` to 0.4.9.

## [0.7.1] - 2024-11-19

### Changed

- Update `proton-crypto` to 0.4.9.

## [0.7.0] - 2024-10-22

### Changed

- Replaced `UnlockedUserKeys`, `UnlockedAddressKeys` type aliases with actual structs providing helper methods. 
- Update `proton-crypto` to 0.4.8.

## [0.6.3] - 2024-10-02

### Changed

- Adapt key secret extraction to new `SRPProvider` version.
- Update `proton-crypto` to 0.4.7.

### Fixed

-  Fix encryption preferences `encrypt` setting for external users with API keys.

## [0.6.2] - 2024-09-11

### Dependencies

- update proton-crypto to 0.4.6


## [0.6.1] - 2024-08-26

### Added 

- `RecipientPublicKeyModel`: Serves as an intermediary type that mirrors vCard content alongside public key information retrieved from the API. This model facilitates the creation of encryption and send preferences.
  
### Changed

- Replace `proton-sql` with `rusqlite`.

## [0.6.0] - 2024-08-13

### Changed

- Add ToSql and FromSql traits to ContactCardType.  Introduce new, "sql" feature flag for crate and move all ToSql/FromSql implementations to be behind the flag (#101)

### Dependencies

- update proton-crypto to 0.4.5

## [0.5.1] - 2024-07-31

### Dependencies

- update proton-crypto to 0.4.4

## [0.5.0] - 2024-07-30

### Changed

- Changed `AttachmentDecryption` to accepts options as `Option<&T>`.

## [0.4.2] - 2024-07-23

### Added

- Add rusqlite ToSql and FromSql traits to the string_id! macro (#92)

## [0.4.1] - 2024-07-22

### Added

- ET-781: encrypt and sign vcards (#81)
- Change locking variant of an existing key (#80)
- Generate singed key lists (SKL) (#82)

## [0.4.0] - 2024-06-26


