include!(concat!(env!("OUT_DIR"), "/package_version.rs"));

/// Return major component of version string of the rust sdk compiled in this artifact.
#[uniffi_export]
#[must_use]
pub fn rust_sdk_version_major() -> u32 {
    VERSION_MAJOR
}

/// Return minor component of version string of the rust sdk compiled in this artifact.
#[uniffi_export]
#[must_use]
pub fn rust_sdk_version_minor() -> u32 {
    VERSION_MINOR
}

/// Return patch component of version string of the rust sdk compiled in this artifact.
#[uniffi_export]
#[must_use]
pub fn rust_sdk_version_patch() -> u32 {
    VERSION_PATCH
}

/// Return the version string of the rust sdk compiled in this artifact.
#[uniffi_export]
#[must_use]
pub fn rust_sdk_version() -> String {
    VERSION_STRING.to_owned()
}
