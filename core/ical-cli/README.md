# ical-cli

Tool for working with *.ics files, useful for debugging the `proton-ical` crate.

## Usage

### CLI

``` bash
$ cargo run -p proton-ical-cli -- check ./examples
```

``` bash
$ cargo run -p proton-ical-cli -- check ./examples/good-enough.ics
```

``` bash
$ cargo run -p proton-ical-cli -- print ./examples/good-enough.ics
```

## FAQ

### Why a separate crate?

So that `proton-ical` doesn't have to directly depend on `anyhow` and `clap`.

This can be also solved via optional dependencies, but `cargo run --feature cli
--bin proton-ical` would be more awkward to use.
