# Rust Build

Collection of build script which help us to build rust for all platforms.

## Rust GitLab CI templates

The `rust-ci.yml` contains useful gitlab ci templates for rust projects.
Can be included with:
```
include:
  - project: "rust/rust-build"
    file: "rust-ci.yml"
```
