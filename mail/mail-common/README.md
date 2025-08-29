# Benchmarks

## Default
To run the benchmark of choice:

```
cargo bench --bench $NAME
```
e.g:
```
cargo bench --bench messages_load
```
## Flamegraph

To generate a flamegraph for the benchmark:

```
cargo bench --bench messages_load -- --profile-time=5
```

An svg will be generated at:
```
target/criterion/$BENCH_NAME/profile/flamegraph.svg

```

## Android

You can run the default benchmarks on android, but we can't generate the flamegraphs.

Generate the binary, executing it will fail, but we can retrieve the binary to execute.
```
 cargo ndk -t arm64-v8a bench --bench messages_load
 ...
     Finished `bench` profile [optimized + debuginfo] target(s) in 2m 01s
     Running benches/messages_load.rs (target/aarch64-linux-android/release/deps/messages_load-ae3f593a25d56ab8)
/Users/user/Repos/proton-rust/target/aarch64-linux-android/release/deps/messages_load-ae3f593a25d56ab8: /Users/user/Repos/proton-rust/target/aarch64-linux-android/release/deps/messages_load-ae3f593a25d56ab8: cannot execute binary file
error: bench failed, to rerun pass `-p proton-mail-common --bench messages_load`
```

Copy the binary and `libgopenpgp-sys.so` from the to `/data/local/tmp`.

```
adb push target/aarch64-linux-android/release/deps/messages_load-ae3f593a25d56ab8 target/aarch64-linux-android/release/libgopenpgp-sys.so /data/local/tmp
```

Then navigate to it and execute

```
adb shell
cd /data/local/tmp
./messages_load-ae3f593a25d56ab8 --bench
```



