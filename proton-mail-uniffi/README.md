# Proton Mail Uniffi

Binding layer for `proton-mail-common` which generates bindings for Kotlin
and Swift.

## Build Setup

### General

* Go v1.22
    * Newer versions of Go may also work, but CI is using last v1.22 release.
* libclang and libllvm
    * Install via Brew or preferred package manager.
* Rust 1.81

### Android

* Linux or Mac Host
    * It should in theory be possible to build this on windows as well
    * All the scripts assume an Unix compatible enviroment is present
* Android SDK
* Android NDK 25c
    * 26 Should also work
    * 27 has never been tested before
* Rust targets:
    * aarch64-linux-android
    * armv7-linux-androideabi
    * x86_64-linux-android
* Cargo NDK (`cargo install cargo-ndk`)
* Maven (mvn)
* Java 17

### iOS

* Mac Host
* XCode
* XCode command line tools
* iOS SDKs
    * Required developer access
* Rust targets:
    * aarch64-apple-ios
    * aarch64-apple-ios-sim
    * x86_64-apple-ios

## Building

The two target artifacts are controlled via the build scripts in the `rust-build`
folder. This folder is a submodule which points to the
[rust-build repository](https://gitlab.protontech.ch/rust/rust-build).

The following instructions are not generic and are tailored to the existing
Android and iOS applications.

### Android

There are 2 ways to build for android, using docker images or with native tools.

See [this confluence page for details](https://confluence.protontech.ch/display/INBOX/How+To%3A+Build+Android+App+targeting+local+Rust+Library).

### iOS

Use the build script as below and define the location of the checked out iOS
application:

```bash
IOS_ROOT_REPO=${PATH_TO_REPO} ./proton-mail-uniffi/ios/build-local.sh
```


