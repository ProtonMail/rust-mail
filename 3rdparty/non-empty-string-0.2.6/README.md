# Non Empty String

[![Crates.io](https://img.shields.io/crates/v/non-empty-string.svg)](https://crates.io/crates/non-empty-string)
[![Documentation](https://docs.rs/non-empty-string/badge.svg)](https://docs.rs/non-empty-string/)

A simple wrapper type for `String`s that ensures that the string inside is not `.empty()`, meaning that the length > 0.

## Example

```rust
use non_empty_string::NonEmptyString;

// Constructing it from a normal (non-zero-length) String works fine.
let s = "A string with a length".to_owned();
assert!(NonEmptyString::new(s).is_ok());

// But constructing it from a zero-length String results in an `Err`, where we get the `String` back that we passed in.
let empty = "".to_owned();
let result = NonEmptyString::new(empty);
assert!(result.is_err());
assert_eq!(result.unwrap_err(), "".to_owned())

```

## Methods of std::string::String

`NonEmptyString` implements a subset of the functions of `std::string::String`, only the ones which are guaranteed to leave the `NonEmptyString` in a valid state.
This means i.e. `push()` is implemented, but `pop()` is not.

This allows you to mostly treat it as a String without having to constantly turn it into the inner `String` before you can do any sort of operation on it and then having to reconstruct a `NonEmptyString` afterwards.

If a method is missing that you think upholds the invariant of `NonEmptyString`, don't hesitate to [open an issue].

## Traits

`NonEmptyString` implements quite a few of the traits that `String` implements, where it simply forwards it to the underlying string,
which allows e.g. indexing with ranges:

```rust
use non_empty_string::NonEmptyString;

let non_empty = NonEmptyString::new("ABC".to_owned()).unwrap();
assert_eq!(&non_empty[1..], "BC");

```

If a trait is missing that you think upholds the invariant of `NonEmptyString`, don't hesitate to [open an issue].

## Serde Support

[serde] support is available behind the `serde` feature flag:

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
non-empty-string = { version = "*", features = ["serde"]}
```

Afterwards you can use it in a struct:

```ignore
use serde::{Serialize, Deserialize};
use non_empty_string::NonEmptyString;

#[derive(Serialize, Deserialize)]
struct MyApiObject {
  username: NonEmptyString,
}
```

Deserialization will fail if the field is present as a String, but the length of the `String` is 0.

## License

Licensed under either of

- Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license
  ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

[serde]: https://docs.rs/serde
[open an issue]: https://github.com/MidasLamb/non-empty-string/issues
