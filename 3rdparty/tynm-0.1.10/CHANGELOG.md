# Changelog

## 0.1.10 (2024-02-12)

* Add `TypeNameInfo` gated behind `"info"` and `"serde"` features. ([#16])

[#16]: https://github.com/azriel91/tynm/pulls/16


## 0.1.9 (2023-09-28)

* Add `tynm::type_name*_opts` methods, taking in `TypeParamsFmtOpts` to specify how type parameters are formatted. ([#15])

[#15]: https://github.com/azriel91/tynm/pulls/15


## 0.1.8 (2023-05-26)

* Support `#![no_std]`. ([#13], [#14])

[#13]: https://github.com/azriel91/tynm/issues/13
[#14]: https://github.com/azriel91/tynm/pulls/14

## 0.1.7 (2023-01-27)

* Update `nom` to `7.1.3` to account for [rust#79813].

[#12]: https://github.com/azriel91/tynm/pulls/12
[rust#79813]: https://github.com/rust-lang/rust/issues/79813

## 0.1.6 (2020-10-16)

* Support creating `TypeName` from `dyn Trait`.

## 0.1.5 (2020-10-13)

* Support parsing `dyn Trait`. ([#9], [#10])

[#9]: https://github.com/azriel91/tynm/issues/9
[#10]: https://github.com/azriel91/tynm/pulls/10

## 0.1.4 (2020-03-07)

* Don't overflow when segment count exceeds `usize::MAX`. ([#7])
* Build `nom` without `"lexical"` feature.

[#7]: https://github.com/azriel91/tynm/pulls/7

## 0.1.3 (2020-01-27)

* Use `unimplemented!` macro instead of `todo!` to support Rust 1.38.0. ([#5])

[#5]: https://github.com/azriel91/tynm/pulls/5

## 0.1.2 (2020-01-10)

* `TypeName::new::<T>()` returns a `TypeName` instance for `T` without constructing the String. ([#3])
* `TypeName::as_display()` and `TypeName::as_display_mn()` both return a `TypeNameDisplay`, allowing one to pass around a `Display` object. ([#3])

[#3]: https://github.com/azriel91/tynm/pulls/3

## 0.1.1 (2020-01-02)

* Support named primitive types (`usize`, `u*`, ..). ([#1])
* Support arrays, slices, and tuples. ([#1])

[#1]: https://github.com/azriel91/tynm/issues/1

## 0.1.0 (2019-12-30)

* `tynm::type_name` returns the simple type name.
* `tynm::type_namem` returns the type name with a chosen number of most significant module segments.
* `tynm::type_namen` returns the type name with a chosen number of least significant module segments.
* `tynm::type_namemn` returns the type name with a chosen number of most and least significant module segments.
