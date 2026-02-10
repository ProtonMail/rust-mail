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

Our application requires XCode 16.3.

Use the build script as below and define the location of the checked out iOS
application:

```bash
IOS_ROOT_REPO=${PATH_TO_REPO} ./proton-mail-uniffi/ios/build-local.sh
```

## Creating a new release

CI is currently setup to publish a new release for mobile when a new tag is
published.

The tags match the version of this crate.

1. Update Cargo.toml version
2. Insert a version entry into the Changelog
    * E.g: `## [0.11.99] - 2024-09-12`
    * Make sure there is an entry with `## [unreleased] - 2024-00-00` on the top
      for new changes after the release.
3. Commit with `chore: Bump proton-mail-uniffi` and push to master
    * If you don't have permissions to merge to master open a pull request
    * Contact a maintainer otherwise
4. Create a new tag (e.g: `v0.11.99`)
5. Push tag
6. Observe release pipeline that is created for the tag
    * You can observe running pipelines with `Build > Pipelines` from the repo
      menu.
7. Once both Android and iOS artifacts are published announce the new release
   in the `#et-releases` slack channel and include a copy of the changelog
   for the released version.
