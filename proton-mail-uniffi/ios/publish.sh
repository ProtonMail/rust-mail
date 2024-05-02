#!/bin/bash

set -eo pipefail

# Read the Rust crate version

cargo generate-lockfile 
CRATE_VERSION=$(cargo pkgid --manifest-path=proton-mail-uniffi/Cargo.toml | cut -d "#" -f2)
echo $CRATE_VERSION

cd ./tmp/ios-framework/

# Compress the folder to upload

FILE_TO_PUBLISH="proton-mail-uniffi-"$CRATE_VERSION".tar.gz"
tar czf $FILE_TO_PUBLISH $CRATE_VERSION

if [ ! -e "$FILE_TO_PUBLISH" ]; then
    echo "Error: File to upload $FILE_TO_PUBLISH does not exist."
    exit 1
fi

# Upload file to the iOS registry

REGISTRY_DEST=$CI_API_V4_URL/projects/3398/packages/generic/rust-sdk/$CRATE_VERSION/$FILE_TO_PUBLISH
echo $REGISTRY_DEST

RESPONSE_CODE=$(curl -s -o /dev/null -w "%{http_code}" --header "PRIVATE-TOKEN: $iOSRegistryAccessToken" --upload-file $FILE_TO_PUBLISH "$REGISTRY_DEST")

if [ "$RESPONSE_CODE" -ne 201 ]; then
    echo "Error: Registry request HTTP response code is "$RESPONSE_CODE"."
    exit 1
fi