fn main() {
    setup_x86_64_android_workaround();
}

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

        const DEFAULT_CLANG_VERSION: &str = "14.0.7";
        let clang_version =
            std::env::var("NDK_CLANG_VERSION").unwrap_or_else(|_| DEFAULT_CLANG_VERSION.to_owned());

        // let cc_var_name = format!();
        // let android_cc = PathBuf::from(env::var("CC_x86_64").expect("Failed to to get cc var"))
        //     .parent()
        //     .unwrap()
        //     .join(format!("{compiler_abi}{platform}-clang"));

        let linux_x86_64_lib_dir = format!(
            "toolchains/llvm/prebuilt/{build_os}-x86_64/lib64/clang/{clang_version}/lib/linux/"
        );
        println!("cargo:rustc-link-search={android_ndk_home}/{linux_x86_64_lib_dir}");
        println!("cargo:rustc-link-lib=static=clang_rt.builtins-x86_64-android");
    }
}
