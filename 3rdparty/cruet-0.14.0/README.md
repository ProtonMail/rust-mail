# Rust Inflector

This project is forked from https://github.com/whatisinternet/Inflector. The original author doesn't maintain this anymore.

[![Build Status](https://travis-ci.org/chrislearn/cruet.svg?branch=master)](https://travis-ci.org/chrislearn/cruet) [![Crates.io](https://img.shields.io/crates/v/cruet.svg)](https://crates.io/crates/cruet)[![Crate downloads](https://img.shields.io/crates/d/cruet.svg)](https://crates.io/crates/cruet)


Adds String based inflections for Rust. Snake, kebab, train, camel,
sentence, class, and title cases as well as ordinalize,
deordinalize, demodulize, deconstantize, foreign key, table case, and pluralize/singularize are supported as both traits and pure functions
acting on &str and String types.

-----
## Documentation:

Documentation can be found here at the README or via rust docs below.

[Rust docs with examples](https://docs.rs/cruet)

-----

## Installation:

### As a [crate](http://crates.io)

```toml
[dependencies]
cruet = "*"
```

### Compile yourself:

1. Install [Rust and cargo](http://doc.crates.io/)
2. git clone https://github.com/chrislearn/cruet
3. Library: cd cruet && cargo build --release --lib
4. You can find the library in target/release

## Usage / Example:

```rust
// to use methods like String.to_lower_case();
use cruet::Inflector;
fn main() {
  let camel_case_string: String = "some_string".to_camel_case();
}

```

Or

```rust
// to use methods like to_snake_case(&str);
use cruet;

// use cruet::to_class_case;
// use cruet::is_class_case;

// use cruet::to_camel_case;
// use cruet::is_camel_case;

// use cruet::to_pascal_case;
// use cruet::is_pascal_case;

// use cruet::to_screamingsnake_case;
// use cruet::is_screamingsnake_case;

// use cruet::to_snake_case;
// use cruet::is_snake_case;

// use cruet::to_kebab_case;
// use cruet::is_kebab_case;

// use cruet::to_train_case;
// use cruet::is_train_case;

// use cruet::to_sentence_case;
// use cruet::is_sentence_case;

// use cruet::to_title_case;
// use cruet::is_title_case;

// use cruet::to_table_case;
// use cruet::is_table_case;

// use cruet::ordinalize;
// use cruet::deordinalize;

// use cruet::to_foreign_key;
// use cruet::is_foreign_key;

// use cruet::demodulize;
// use cruet::deconstantize;

// use cruet::to_plural;
// use cruet::to_singular;
fn main() {
  let camel_case_string: String = to_camel_case("some_string");
}
```

## [Contributing](CONTRIBUTING.md)

This project is intended to be a safe, welcoming space for collaboration, and contributors are expected to adhere to the [Contributor Covenant](http://contributor-covenant.org) code of conduct.
