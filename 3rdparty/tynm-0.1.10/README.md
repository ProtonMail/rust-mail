# 🪶 Tynm -- Type Name

[![Crates.io](https://img.shields.io/crates/v/tynm.svg)](https://crates.io/crates/tynm)
[![docs.rs](https://img.shields.io/docsrs/tynm)](https://docs.rs/tynm)
[![CI](https://github.com/azriel91/tynm/workflows/CI/badge.svg)](https://github.com/azriel91/tynm/actions/workflows/ci.yml)
[![Coverage Status](https://codecov.io/gh/azriel91/tynm/branch/main/graph/badge.svg)](https://codecov.io/gh/azriel91/tynm)

Returns type names with a specifiable number of module segments as a `String`.

## Usage

Add the following to `Cargo.toml`

```toml
tynm = "0.1.10"
```

In code:

```rust
#[rustfmt::skip]
assert_eq!(
    core::any::type_name::<Option<String>>(), "core::option::Option<alloc::string::String>"
);

#[rustfmt::skip]
let tuples = vec![
    (tynm::type_name::<Option<String>>(),    "Option<String>"),
    (tynm::type_namem::<Option<String>>(1),  "core::..::Option<alloc::..::String>"),
    (tynm::type_namen::<Option<String>>(1),  "..::option::Option<..::string::String>"),
    // 1 segment from most and least significant modules.
    (tynm::type_namemn::<rust_out::two::three::Struct>(1, 1), "rust_out::..::three::Struct"),
    // traits
    (tynm::type_name::<dyn core::fmt::Debug>(), "dyn Debug"),
];

tuples
    .iter()
    .for_each(|(left, right)| assert_eq!(left, right));

```

## Motivation

The [`core::any::type_name`] function stabilized in Rust 1.38 returns the fully qualified type
name with all module segments. This can be difficult to read in error messages, especially for
type-parameterized types.

Often, the simple type name is more readable, and enough to distinguish the type referenced in
an error.

[`core::any::type_name`]: https://doc.rust-lang.org/std/any/fn.type_name.html

## License

Licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
