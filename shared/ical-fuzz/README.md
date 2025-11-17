# ical-fuzz

Fuzzing suite for the ical crate.

## Usage

1. Install [`cargo-fuzz`](https://rust-fuzz.github.io/book/cargo-fuzz.html).
2. Enjoy:

``` bash
$ cd shared/ical-fuzz

$ rustup default nightly
# ^ if you're using devenv, remember to comment-out rustc in there, so that
#   the rustup override here is actually applied

$ cargo fuzz run main --fuzz-dir=. -- -timeout=1s -max_len=32000
```

Once the fuzzer finds something interesting (i.e. parser crashes or timeouts),
it will stop and say:

```
==73821== ERROR: libFuzzer: timeout after 1 seconds
    #0 0x0001057d9cc0 in __sanitizer_print_stack_trace+0x28 (librustc-nightly_rt.asan.dylib:arm64+0x5dcc0)
    #1 0x000104e15244 in fuzzer::PrintStackTrace()+0x30 (main:arm64+0x1003b5244)
    [...]

SUMMARY: libFuzzer: timeout

────────────────────────────────────────────────────────────────────────────────

Failing input:

        ./artifacts/main/...
```

... in which case the `./arfitacts/main/...` file will contain the *.ics content
that caused the parser to fail.

### Corpus

By default, the fuzzer starts with no knowledge about the *.ics files - it tries
to guess how the format looks like. This makes the fuzzing a bit less effective
that it can be - you can help the fuzzer by preseeding the corpus with a couple
of known files, e.g. from the ical tests:

``` bash
$ cd shared/ical-fuzz
$ mkdir -p corpus/main
$ find ../ical -type f -name '*.ics' -exec bash -c 'cp {} corpus/main/$(sha256sum {} | awk "{ print \$1 }")' \;
```
