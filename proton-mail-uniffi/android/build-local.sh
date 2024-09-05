#
# Build android artifact locally
#
# Run from the root of the repository
#
# ./proton-mail-uniffi/android/build-local.sh
set -eo pipefail


# Build code
rust-build/build_android.sh proton-mail-uniffi proton-mail-uniffi/uniffi.toml ./proton-mail-uniffi/android/lib/src/main/
# Build archive
./proton-mail-uniffi/android/build-android-archive.sh
rm -rf /tmp/rust-builds
mkdir /tmp/rust-builds/
# Copy artifacts
cp ./proton-mail-uniffi/android/lib/build/outputs/aar/lib-release.aar /tmp/rust-builds/
# Pubish
CRATE_VERSION=$(cargo pkgid --manifest-path=proton-mail-uniffi/Cargo.toml | cut -d "#" -f2)
mvn install:install-file -Dfile=/tmp/rust-builds/lib-release.aar -DgroupId=me.proton.mail.common -DartifactId=lib -Dversion=$CRATE_VERSION -Dpackaging=aar
