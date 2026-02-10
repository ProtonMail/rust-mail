use std::path::PathBuf;

fn main() {
    gen_package_version_info();
    setup_x86_64_android_workaround();
}

const DEFAULT_CLANG_VERSION: &str = "19";
fn setup_x86_64_android_workaround() {
    // FIXME: hack to ensure that libs compile correctly for android x86_64bit emulator versions
    //        see https://github.com/rusqlite/rusqlite/issues/1380#issuecomment-1689765485
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS not set");
    let target_arch =
        std::env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH not set");
    if target_arch == "x86_64" && target_os == "android" {
        let android_ndk_home = std::env::var("ANDROID_NDK_HOME").expect("ANDROID_NDK_HOME not set");
        let build_os = match std::env::consts::OS {
            "linux" => "linux",
            "macos" => "darwin",
            "windows" => "windows",
            _ => panic!(
                "Unsupported OS. You must use either Linux, MacOS or Windows to build the crate."
            ),
        };

        let mut ndk_path = PathBuf::new();
        ndk_path.push(android_ndk_home.clone());
        ndk_path.push("toolchains");
        ndk_path.push("llvm");
        ndk_path.push("prebuilt");
        ndk_path.push(format!("{build_os}-x86_64"));

        let mut linux_x86_64_lib_dir = ndk_path.join("lib");
        linux_x86_64_lib_dir.push("clang");

        let clang_version =
            std::env::var("NDK_CLANG_VERSION").unwrap_or_else(|_| DEFAULT_CLANG_VERSION.to_owned());
        linux_x86_64_lib_dir.push(&clang_version);
        linux_x86_64_lib_dir.push("lib");
        linux_x86_64_lib_dir.push("linux");

        assert!(
            linux_x86_64_lib_dir.exists(),
            "clang path not known! Is the ndk version 28?"
        );

        println!(
            "cargo:rustc-link-search={}",
            linux_x86_64_lib_dir.to_string_lossy()
        );
        println!("cargo:rustc-link-lib=static=clang_rt.builtins-x86_64-android");
    }
}

// Generate package version info that can be accessed at runtime.
fn gen_package_version_info() {
    let major = std::env::var("CARGO_PKG_VERSION_MAJOR").expect("CARGO_PKG_VERSION_MAJOR not set");
    let minor = std::env::var("CARGO_PKG_VERSION_MINOR").expect("CARGO_PKG_VERSION_MINOR not set");
    let patch = std::env::var("CARGO_PKG_VERSION_PATCH").expect("CARGO_PKG_VERSION_PATCH not set");

    let output_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set"))
        .join("package_version.rs");

    let data = format!(
        r#"
pub const VERSION_MAJOR:u32 = {major};
pub const VERSION_MINOR:u32 = {minor};
pub const VERSION_PATCH:u32 = {patch};
pub const VERSION_STRING:&str = "{major}.{minor}.{patch}";
    "#
    );

    std::fs::write(&output_dir, data).expect("Could not write to version file");
}
