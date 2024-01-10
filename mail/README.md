# rust-template

This repo contains a quick template on how to hook up a rust project to CI. This readme also
contains additional information on how to compile rust for different platforms.


## Web Assembly (WASM)

For WASM the project needs to be compiled with `wasm-pack`, available in the docker image for this
template.

It's recommended to feature gate the binding generation code and only enable it when
actively compiling for these targets.

In your crate simply run:

```
wasm-pack build -d $OUTPUT --target web --features="feature1,feature2"
```

## Mobile

To generate bindings for mobile you need to use [uniffi-rs](https://github.com/mozilla/uniffi-rs).

Please use the [procedural macro
API](https://mozilla.github.io/uniffi-rs/proc_macro/index.html), this makes it easier to generate
the final binding set for the application.

Generating the bindings is a 2 step process. First, you need to generate the final dynamic library
for the application and then run the `uniffi-bindgen` command to generate the bindings for the
target language.

Finally it's recommended to feature gate the binding generation code and only enable it when
actively compiling for these targets.

### Android

**IMPORTANT:** Be sure to read the [Kotlin Lifetime chapter](https://mozilla.github.io/uniffi-rs/kotlin/lifetimes.html) to avoid memory leaks.

#### Generate Kotlin bindings

```
cargo run  --release -p uniffi-bindgen generate --library $PATH_TO_SHARED_LIBRARY --language kotlin --out-dir $OUT_DIR
```

#### Integration with Android Project

There are different ways to integrate the cargo build process:

* [rust android gradle](https://github.com/mozilla/rust-android-gradle)
* [uniffi-rs docs](https://mozilla.github.io/uniffi-rs/kotlin/gradle.html)

For rust-android-gradle you just need the add the following snippets to the app's `build.gradle`
file and replace as needed.
```
cargo {
    module = "../../rust/mailbox/mailbox-ffi"
    libname = "mailbox_ffi"
    targets = ["x86_64", "arm64"]
    targetIncludes = ["libmailbox_ffi.so", "libgopenpgp-sys.so"]
    targetDirectory = "../../rust/mailbox/target"
    profile = "release"
    features {
        noDefaultBut()
    }
}

task genKotlinBindings(type: Exec) {
    String rustFolder = "${project.getProjectDir()}/../../rust"
    workingDir rustFolder
    //TODO: Update this per CPU architecture.
    commandLine "cargo", "run", "--release" ,"-p", "uniffi-bindgen", "generate", "--library", ${rustFolder}/target/aarch64-linux-android/release/libmailbox_uniffi.so","--language", "kotlin", "--out-dir", "${project.getProjectDir()}/src/main/java"
}
genKotlinBindings.dependsOn 'cargoBuild'

tasks.whenTaskAdded { task ->
    //TODO: Cargo clean on project clean
    // Require cargo to be run before copying native libraries.
    if ((task.name == 'mergeDebugJniLibFolders' || task.name == 'mergeReleaseJniLibFolders')) {
        task.dependsOn 'cargoBuild'
    }

    if ((task.name == 'javaPreCompileDebug' || task.name == 'javaPreCompileRelease')) {
        task.dependsOn 'cargoBuild'
        task.dependsOn genKotlinBindings
    }

    if ((task.name == 'compileReleaseKotlin' || task.name == 'compileDebugKotlin')) {
        task.dependsOn 'cargoBuild'
        task.dependsOn genKotlinBindings
    }
}

afterEvaluate {
    // The `cargoBuild` task isn't available until after evaluation.
    android.applicationVariants.all { variant ->
        def productFlavor = ""
        variant.productFlavors.each {
            productFlavor += "${it.name.capitalize()}"
        }
        def buildType = "${variant.buildType.name.capitalize()}"
        tasks["generate${productFlavor}${buildType}Assets"].dependsOn(tasks["cargoBuild"])
    }
}




```

Additionally you also need to specify the NDK version and the NDK target architectures:

```
android {
    ndkVersion "26.1.10909125"

    defaultConfig {
        ndk {
            //noinspection ChromeOsAbiSupport
            abiFilters 'arm64-v8a', 'x86_64'
        }
    }

}
```

#### Building on CI

Due to the crypto code that is still in go, building for android on ci is not as straight forward.

You need to set a bunch of environment variables so that the go code can be compiled correctly.

The snippet below demonstrates how to set up the environment variables for aarch64 and x86_64. Note
that rust-android-gradle sets these for you.
```
    if [ "$2" == "arm64-v8a" ]; then
        CARGO="env CC_aarch64-linux-android=${android_tools}/aarch64-linux-android21-clang"
        CARGO="${CARGO} CC=${android_tools}/aarch64-linux-android21-clang"
        CARGO="${CARGO} AR_aarch64-linux-android=$android_tools/llvm-ar"
        CARGO="${CARGO} CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$android_tools/aarch64-linux-android21-clang"
        CARGO_TARGET="--target=aarch64-linux-android"
        CARGO_TARGET_DIR="target/aarch64-linux-android/release"
    elif [ "$2" == "x86_64" ]; then
        CARGO="env CC_x86_64-linux-android=$android_tools/x86_64-linux-android21-clang"
        CARGO="${CARGO} CC=$android_tools/x86_64-linux-android21-clang"
        CARGO="${CARGO} AR_x86_64-linux-android=$android_tools/llvm-ar"
        CARGO="${CARGO} CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER=$android_tools/x86_64-linux-android21-clang"
        CARGO_TARGET="--target=x86_64-linux-android"
        CARGO_TARGET_DIR="target/x86_64-linux-android/release"
    else
        echo "Invalid android arch, please pick arm64-v8a or x86_64"
        popd
        exit -1
    fi
```

### IOS

#### Generate Swift bindings
```
cargo run  --release -p uniffi-bindgen generate --library $PATH_TO_SHARED_LIBRARY --language swift --out-dir $OUT_DIR
```

#### Generating an xcframework

The snippet below illustrates the necessary steps required to generate an xcframework that can be
used with the iOS application.

Note that this snippet only handles the generated binaries, it needs to be extended to include the

```
    echo "Building x86_64 Sim"
    cargo build --release -p mailbox-ffi ${CARGO_TARGET} --target x86_64-apple-ios
    check_exit

    echo "Building aarch64 Sim"
    cargo build --release -p mailbox-ffi ${CARGO_TARGET} --target aarch64-apple-ios-sim
    check_exit

    echo "Building aarch64"
    cargo build --release -p mailbox-ffi ${CARGO_TARGET} --target aarch64-apple-ios
    check_exit

    mkdir -p "$CP_DIR/ios-sim"
    check_exit

    mkdir -p "$CP_DIR/ios-dev"
    check_exit

    echo "Generating universal sim universal binary"
    lipo -create -output "$CP_DIR/ios-sim/libmailbox_ffi_sim.dylib" \
"target/x86_64-apple-ios/release/libmailbox_ffi.dylib" \
"target/aarch64-apple-ios-sim/release/libmailbox_ffi.dylib"
    check_exit

    cp "target/aarch64-apple-ios/release/libmailbox_ffi.dylib" "$CP_DIR/ios-dev/libmailbox_ffi_dev.dylib"
    check_exit

    xcodebuild -create-xcframework \
        -output "$CP_DIR/libmailbox_ffi.xcframework" \
        -library "$CP_DIR/ios-sim/libmailbox_ffi_sim.dylib" \
        -library "$CP_DIR/ios-dev/libmailbox_ffi_dev.dylib"
    check_exit
```
