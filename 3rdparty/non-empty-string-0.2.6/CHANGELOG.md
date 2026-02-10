# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.6](https://github.com/MidasLamb/non-empty-string/compare/v0.2.5...v0.2.6) - 2025-04-09

### Added

- `non_empty_string!()` macro to create a `NonEmptyString` at compile time , thanks @patskovn in #19 (requires `macros` feature flag)

## [0.2.5](https://github.com/MidasLamb/non-empty-string/compare/v0.2.4...v0.2.5) - 2024-10-15

### Added

- `new_unchecked` (Thanks @Rudxain in #17 )

## [0.2.4](https://github.com/MidasLamb/non-empty-string/compare/v0.2.3...v0.2.4) - 2023-10-03

### Added

- impl Hash trait
- impl Into trait
- FromStr implementation
- add Deref impl

### Other

- Add release plz
- Remove excessive error file
- Add more traits that `String` also has
- Add cargo-semver-checks to CI
- error::EmptyString is now public
- Merge branch 'MidasLamb:master' into constructors
- cover parse() in tests::from_str_works
- fix example description about constructor
- clear unnecessary paths
- Release version v0.2.3

### Added

- More traits from `String` implemented on `NonEmptyString`
  - Index
  - Add
  - AddAssign
  - Extend
  - Write
  - PartialEq with `str` & `String`

### Changed

### Removed

## [0.2.3]

### Added

- Add `Display` implementation ([#8](https://github.com/MidasLamb/non-empty-string/pull/8), thanks to [@jonhteper](https://github.com/jonhteper))
- Align errors in `TryFrom` implementations ([#8](https://github.com/MidasLamb/non-empty-string/pull/8), thanks to [@jonhteper](https://github.com/jonhteper))

## [0.2.2]

### Added

- Add & delegate all non-length-reducing methods of `std::string::String` to the inner `String`.

### Changed

- README has some more examples and explanations. It is also no longer included in the doc (except for doctests).

## [0.2.1]

### Changed

- The error message when using `serde` now indicates that the empty string could not be deserialized.
- Bumped rust edition to `2021`

## [0.2.0]

### Added

- `serde` support behind the `serde` feature flag.
- `Eq, PartialEq, Ord, PartialOrd` are now implemented for `NonEmptyString`.
- `get` to retrieve a reference to the inner value.

### Changed

- `new` constructor now returns a `Result` rather than an `Option`, which contains the original string

[unreleased]: https://github.com/MidasLamb/non-empty-string/compare/v0.2.3...HEAD
[0.2.3]: https://github.com/MidasLamb/non-empty-string/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/MidasLamb/non-empty-string/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/MidasLamb/non-empty-string/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/MidasLamb/non-empty-string/compare/v0.1.0...v0.2.0
