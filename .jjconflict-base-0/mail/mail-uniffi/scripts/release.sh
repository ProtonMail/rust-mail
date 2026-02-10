#!/usr/bin/env bash

# Performs a release with a version bump specified by the first argument.
#
# A minor bump (e.g. 0.126.0 -> 0.127.0)
# sh mail/mail-uniffi/scripts/release.sh minor
#
# A patch bump (e.g. 0.126.0 -> 0.126.1)
# sh mail/mail-uniffi/scripts/release.sh patch

set -euo pipefail

toml() {
  uvx --from toml-cli toml "$@"
}

semver() {
  uvx --from semver pysemver "$@"
}

echo "Bumping..."
CURR="$(toml get package.version --toml-path ./mail/mail-uniffi/Cargo.toml)"
NEXT="$(semver bump "$1" "$CURR")"
toml set package.version "$NEXT" --toml-path ./mail/mail-uniffi/Cargo.toml

echo "Releasing..."
cargo check
sh ./mail/mail-uniffi/scripts/gen_changelog.sh --name "mail-uniffi-v$NEXT"
git commit -a -m "chore: mail-uniffi v$NEXT"
