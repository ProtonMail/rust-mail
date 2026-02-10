#!/bin/sh

# Build iOS XCFramework for UniFFI bindings
#
# Usage: $0 rust_target config_path [output_dir] [profile]
#
# Arguments:
#   rust_target  - Name of the Rust crate to build (e.g. proton-mail-uniffi)
#   config_path  - Path to uniffi.toml configuration file
#   output_dir   - Optional: Output directory (defaults to current directory)
#   profile      - Optional: Cargo profile to use (defaults to "ios" for release)
#
# Examples:
#   # Release build (optimized, no debug symbols)
#   $0 proton-mail-uniffi ./mail/mail-uniffi/uniffi.toml "./tmp/ios-framework"
#
#   # Debug build (with debug symbols for LLDB/RustRover debugging)
#   $0 proton-mail-uniffi ./mail/mail-uniffi/uniffi.toml "./tmp/ios-framework-debug" ios-debug

MIN_IOS_VERSION="17.2"
PROFILE="${4:-ios}" # Use 4th argument if provided, otherwise default to "ios"

export LIBSQLITE3_FLAGS="-DNDEBUG=1 -DSQLITE_THREADSAFE=2\
 -DSQLITE_DEFAULT_FILE_PERMISSIONS=0600 -DSQLITE_SECURE_DELETE"


# iOS specific flags

function check_exit {
    retVal=$?
    if [ $retVal -ne 0 ]; then
        echo "Command failed"
        exit $retVal
    fi
}

function check_exit_popd {
    retVal=$?
    if [ $retVal -ne 0 ]; then
        echo "Command failed"
        popd
        exit $retVal
    fi
}


function join_by {
    local IFS="$1"; shift; echo "$*";
}


# gen_framework path name swift_include_dir
function gen_framework {
   local TARGET_DIR=$1
   local NAME=$2
   local FRAMEWORK_NAME="${NAME}_ffi.framework"
   local FRAMEWORK_PATH="$TARGET_DIR/$FRAMEWORK_NAME"
   local SWIFT_HEADER_DIR=$3
   local CRATE_VERSION=$4
   local PLATFORM=$5
   local NAME_PASCAL_CASE=$(echo "${NAME}_ffi" | perl -pe 's/(?:^|_)./uc($&)/ge;s/_//g')

   echo "$FRAMEWORK_PATH"

   find $TARGET_DIR -type d -name "$FRAMEWORK_NAME" -exec rm -rf {} \; || echo "rm failed"

   mkdir -p "$FRAMEWORK_PATH" "$FRAMEWORK_PATH/Headers" "$FRAMEWORK_PATH/Modules" "$FRAMEWORK_PATH/Resources"
   check_exit

   local MIN_VERSION=$MIN_IOS_VERSION

   if [ "$PLATFORM" = "iphonesimulator" ]; then
        MIN_VERSION=100
   fi

   # iOS has Info.plist at the root
   cat > $FRAMEWORK_PATH/Info.plist << CATEOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>me.proton.${NAME_PASCAL_CASE}</string>
    <key>CFBundleName</key>
    <string>${NAME}_ffi</string>
    <key>CFBundleVersion</key>
    <string>${CRATE_VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${CRATE_VERSION}</string>
    <key>CFBundleExecutable</key>
    <string>${NAME}_ffi</string>
    <key>MinimumOSVersion</key>
    <string>${MIN_VERSION}</string>
    <key>CFBundleSupportedPlatforms</key>
    <array>
        <string>$PLATFORM</string>
    </array>
</dict>
</plist>
CATEOF

    # We need to generate a custom module and include the header files ourselves

    echo "framework module ${NAME}_ffi {" > $SWIFT_HEADER_DIR/module.tmp
    for header in $SWIFT_HEADER_DIR/*.h
    do
        local header_name=$(basename "$header")
        echo "    header \"$header_name\"" >> $SWIFT_HEADER_DIR/module.tmp
        check_exit
    done

    echo "    export *\n}" >> $SWIFT_HEADER_DIR/module.tmp

    cp $SWIFT_HEADER_DIR/*.h "$FRAMEWORK_PATH/Headers"
    check_exit

    cp $SWIFT_HEADER_DIR/module.tmp "$FRAMEWORK_PATH/Modules/module.modulemap"
    check_exit

    rmdir $FRAMEWORK_PATH/Resources

    # Additional satitic libraries need to be bundled together
    if [[ -f "${TARGET_DIR}/libgopenpgp-sys.a" ]]; then
        libtool -static -o "$FRAMEWORK_PATH/${NAME}_ffi" $TARGET_DIR/lib${NAME}.a $TARGET_DIR/libgopenpgp-sys.a
        check_exit
    else
        cp $TARGET_DIR/lib${NAME}.a "$FRAMEWORK_PATH/${NAME}_ffi"
        check_exit
    fi
}


function gen_swift_package {
    local NAME=$2
    local FRAMEWORK_NAME=$3
    cat > $1/Package.swift << CATEOF
// swift-tools-version:5.5
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "$NAME",
    platforms: [
      .iOS(.v15),
    ],
    products: [
        // Products define the executables and libraries a package produces, and make them visible to other packages.
        .library(
            name: "$NAME",
            targets: ["$NAME", "${FRAMEWORK_NAME}_ffi"]
        ),
    ],
    dependencies: [
        // Dependencies declare other packages that this package depends on.
        // .package(url: /* package url */, from: "1.0.0"),
    ],
    targets: [
        // Targets are the basic building blocks of a package. A target can define a module or a test suite.
        // Targets can depend on other targets in this package, and on products in packages this package depends on.
        .target(
            name: "$NAME",
            dependencies: ["${FRAMEWORK_NAME}_ffi"]
        ),
         .binaryTarget(name: "${FRAMEWORK_NAME}_ffi", path: "Sources/${FRAMEWORK_NAME}_ffi.xcframework"),
    ]
)

CATEOF
}


SO_SUFFIX="dylib"
TMP_DIR="/tmp/$(uuidgen)"
TARGET=$1
TARGET_UNDERSCORE=$(echo $TARGET | tr "-" "_")
CONFIG_PATH="$2"
OUTPUT_DIR="$3"
if [ "$OUTPUT_DIR" == "" ]; then
    OUTPUT_DIR=$PWD
fi

echo "Creating tmpdir $TMP_DIR"
mkdir -p "$TMP_DIR"
check_exit

TMP_DIR=$(readlink -f "$TMP_DIR")
check_exit

echo "Building aarch64 Sim"
IPHONEOS_DEPLOYMENT_TARGET=$MIN_IOS_VERSION cargo build --profile $PROFILE -p $TARGET --target aarch64-apple-ios-sim
check_exit

echo "Building aarch64"
IPHONEOS_DEPLOYMENT_TARGET=$MIN_IOS_VERSION cargo build --profile $PROFILE -p $TARGET --target aarch64-apple-ios
check_exit

mkdir -p "$TMP_DIR/ios-sim" "$TMP_DIR/ios-sim-swift-x86_64" "$TMP_DIR/ios-sim-swift-aarch64"
check_exit

mkdir -p "$TMP_DIR/ios-dev"
check_exit

mkdir -p "$TMP_DIR/include"
check_exit

echo "Determining crate version"
CRATE_VERSION=$(cargo metadata --format-version 1 --no-deps | jq --arg target "$TARGET" '.packages[] | select(.name == $target) | .version' | jq -r)

echo "Generating swift bindings"
cargo run --release -p uniffi-bindgen generate --library "target/aarch64-apple-ios/$PROFILE/lib${TARGET_UNDERSCORE}.a" \
    --config "$CONFIG_PATH" --language swift --out-dir "$TMP_DIR/include"
check_exit

echo "Generating framework for ios sim aarch64"
gen_framework "target/aarch64-apple-ios-sim/$PROFILE" $TARGET_UNDERSCORE "$TMP_DIR/include" $CRATE_VERSION iphonesimulator

echo "Generating framework for ios aarch64"
gen_framework "target/aarch64-apple-ios/$PROFILE" $TARGET_UNDERSCORE "$TMP_DIR/include" $CRATE_VERSION iphoneos


cp target/aarch64-apple-ios/$PROFILE/lib${TARGET_UNDERSCORE}.a "$TMP_DIR/ios-dev/lib${TARGET_UNDERSCORE}_dev.a"
check_exit


SWIFT_PACKAGE_DIR=$OUTPUT_DIR/${CRATE_VERSION}
SWIFT_PACKAGE_SOURCES_DIR=$SWIFT_PACKAGE_DIR/Sources
FRAMEWORK_NAME="${TARGET_UNDERSCORE}_ffi.xcframework"
PACKAGE_NAME="proton_app_uniffi"
echo "Creating ${SWIFT_PACKAGE_SOURCES_DIR}/$FRAMEWORK_NAME"

mkdir -p $SWIFT_PACKAGE_SOURCES_DIR $SWIFT_PACKAGE_SOURCES_DIR/${TARGET_UNDERSCORE}
check_exit

gen_swift_package $SWIFT_PACKAGE_DIR $PACKAGE_NAME $TARGET_UNDERSCORE


if [[ -d "$SWIFT_PACKAGE_SOURCES_DIR/${FRAMEWORK_NAME}" ]]; then
    echo "Deleting previous artifact"
    rm -r "$SWIFT_PACKAGE_SOURCES_DIR/${FRAMEWORK_NAME}"
    check_exit
fi

echo "Generating xcframework"
xcodebuild -create-xcframework \
    -output  "$SWIFT_PACKAGE_SOURCES_DIR/${FRAMEWORK_NAME}" \
    -framework "target/aarch64-apple-ios-sim/$PROFILE/${TARGET_UNDERSCORE}_ffi.framework" \
    -framework "target/aarch64-apple-ios/$PROFILE/${TARGET_UNDERSCORE}_ffi.framework"
check_exit

echo "Changing folder name"
mv $SWIFT_PACKAGE_SOURCES_DIR/$TARGET_UNDERSCORE $SWIFT_PACKAGE_SOURCES_DIR/$PACKAGE_NAME


# We now need to replace the module imports with the correct module name.

for header in $TMP_DIR/include/*.swift
do
    header_name=$(basename $header)
    header_name_only=$(basename $header ".swift")
    sed "s/${header_name_only}FFI/${TARGET_UNDERSCORE}_ffi/g" $header > $SWIFT_PACKAGE_SOURCES_DIR/${PACKAGE_NAME}/$header_name
    check_exit
done


rm -r "$TMP_DIR"
check_exit
