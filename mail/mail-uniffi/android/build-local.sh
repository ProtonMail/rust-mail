#
# Build android artifact locally
#
# Run from the root of the repository
#
# ./mail/mail-uniffi/android/build-local.sh
set -eo pipefail


# Build code
rust-build/build_android.sh proton-mail-uniffi ./mail/mail-uniffi/uniffi.toml ./mail/mail-uniffi/android/lib/src/main/
# Build archive
./mail/mail-uniffi/android/build-android-archive.sh
rm -rf /tmp/rust-builds
mkdir /tmp/rust-builds/
# Copy artifacts
cp ./mail/mail-uniffi/android/lib/build/outputs/aar/lib-release.aar /tmp/rust-builds/
# Pubish
CRATE_VERSION=$(cargo pkgid --manifest-path=./mail/mail-uniffi/Cargo.toml | cut -d "@" -f2)
mvn install:install-file -Dfile=/tmp/rust-builds/lib-release.aar -DgroupId=me.proton.mail.common -DartifactId=lib -Dversion=$CRATE_VERSION -Dpackaging=aar
