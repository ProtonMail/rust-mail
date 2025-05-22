# ical

Comprehensive iCalendar parser / generator / editor.

- https://www.rfc-editor.org/rfc/rfc5545
- https://www.rfc-editor.org/rfc/rfc5546

## Usage

### CLI

See: <../ical-cli/README.md>.

### Rust

See <./tests/acceptance.rs>

### PHP

``` bash
$ cd core/ical
$ RUSTFLAGS="-C link-arg=-Wl,-undefined,dynamic_lookup" cargo build -p proton-ical --release --features php

# on Linux
$ php -d extension=../../target/release/libical.so examples/php/parse.php
$ php -d extension=../../target/release/libical.so examples/php/print.php
$ php -d extension=../../target/release/libical.so examples/php/trip.php

# on Mac
$ php -d extension=../../target/release/libical.dylib examples/php/parse.php
$ php -d extension=../../target/release/libical.dylib examples/php/print.php
$ php -d extension=../../target/release/libical.dylib examples/php/trip.php
```
