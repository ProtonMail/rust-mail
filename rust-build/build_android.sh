#/bin/bash
set -eu

# usage arg0 target config_path out_dir

# https://android.googlesource.com/platform/external/sqlite/+/refs/heads/main/dist/Android.bp
export LIBSQLITE3_FLAGS="-DNDEBUG=1 -DSQLITE_POWERSAFE_OVERWRITE=1 -DSQLITE_THREADSAFE=2\
 -DSQLITE_DEFAULT_FILE_PERMISSIONS=0600 -DSQLITE_SECURE_DELETE -DSQLITE_ENABLE_BATCH_ATOMIC_WRITE"


TARGET=$1
CONFIG_PATH=$2
OUT_DIR=$3
PROFILE="android"
LIB_NAME="lib$(echo $1 | tr '-' '_').so"

mkdir -p $OUT_DIR/java \
$OUT_DIR/jniLibs/arm64-v8a \
$OUT_DIR/jniLibs/armeabi-v7a \
$OUT_DIR/jniLibs/x86_64 \

# Build project
cargo ndk -t "armeabi-v7a" -t "arm64-v8a" -t "x86_64" build --profile $PROFILE -p $TARGET

# Generate bindings
cargo run \
    --release \
    -p uniffi-bindgen generate \
    --library target/aarch64-linux-android/$PROFILE/${LIB_NAME} \
    --language kotlin \
    --config ${CONFIG_PATH} \
    --out-dir $OUT_DIR/java \
    --no-format

# Strip symbols
OS_NAME=$(uname -s | tr '[:upper:]' '[:lower:]')
STRIP_BIN=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/${OS_NAME}-x86_64/bin/llvm-strip
$STRIP_BIN target/aarch64-linux-android/$PROFILE/$LIB_NAME -o $OUT_DIR/jniLibs/arm64-v8a/$LIB_NAME
$STRIP_BIN target/armv7-linux-androideabi/$PROFILE/$LIB_NAME -o $OUT_DIR/jniLibs/armeabi-v7a/$LIB_NAME
$STRIP_BIN target/x86_64-linux-android/$PROFILE/$LIB_NAME -o $OUT_DIR/jniLibs/x86_64/$LIB_NAME


PGP_SYS_LIB="libgopenpgp-sys.so"
if [[ -f "target/aarch64-linux-android/$PROFILE/$PGP_SYS_LIB" ]]; then
    $STRIP_BIN target/aarch64-linux-android/$PROFILE/$PGP_SYS_LIB -o $OUT_DIR/jniLibs/arm64-v8a/$PGP_SYS_LIB
    $STRIP_BIN target/armv7-linux-androideabi/$PROFILE/$PGP_SYS_LIB -o $OUT_DIR/jniLibs/armeabi-v7a/$PGP_SYS_LIB
    $STRIP_BIN target/x86_64-linux-android/$PROFILE/$PGP_SYS_LIB -o $OUT_DIR/jniLibs/x86_64/$PGP_SYS_LIB
fi
