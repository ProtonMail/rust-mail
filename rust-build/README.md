# Rust Build Scripts

Build scripts for generating platform-specific Rust frameworks.

## iOS Framework Build

### Usage

Build iOS XCFramework with UniFFI bindings:

```bash
# Debug build (with DWARF symbols for debugging)
./rust-build/build_ios_framework_uniffi.sh proton-mail-uniffi ./mail/mail-uniffi/uniffi.toml "./tmp/ios-framework-debug" ios-debug

# Release build (optimized for production)
./rust-build/build_ios_framework_uniffi.sh proton-mail-uniffi ./mail/mail-uniffi/uniffi.toml "./tmp/ios-framework"
```

**Arguments:**
- `rust_target` - Rust crate name (e.g. `proton-mail-uniffi`)
- `config_path` - Path to `uniffi.toml` configuration
- `output_dir` - Output directory (optional, defaults to current directory)
- `profile` - Cargo profile (optional, defaults to `ios` for release)

### Debugging Setup

To debug Rust code in iOS apps:

1. Build debug framework using `ios-debug` profile (as shown above)
2. See the iOS app repository README for LLDB/RustRover debugging setup
3. Set breakpoints and debug with full source code visibility

## Android Build

```bash
./rust-build/build_android.sh
```
