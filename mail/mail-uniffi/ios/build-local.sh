#!/bin/sh

# Build io artifact locally and copy it to the right location in
# the ios repo
#
# Run from the root of the repository
#
# IOS_REPO_ROOT=$PATH ./mail/mail-uniffi/ios/build-local.sh

set -eo pipefail

TMP_DIR="/tmp/$(uuidgen)"

rust-build/build_ios_framework_uniffi.sh proton-mail-uniffi ./mail/mail-uniffi/uniffi.toml $TMP_DIR

CRATE_VERSION=$(cargo pkgid --manifest-path=./mail/mail-uniffi/Cargo.toml | cut -d "@" -f2)

cp -r $TMP_DIR/$CRATE_VERSION/* $IOS_REPO_ROOT/ProtonPackages/proton_app_uniffi/

