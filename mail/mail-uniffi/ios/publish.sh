#!/bin/bash

set -eo pipefail

# Read the Rust crate version

CRATE_VERSION=$(cargo metadata --locked --format-version 1 --no-deps | jq '.packages[] | select(.name == "proton-mail-uniffi") | .version' | jq -r)
echo $CRATE_VERSION

cd ./tmp/ios-framework/

# Compress the folder to upload

FILE_TO_PUBLISH="proton-mail-uniffi-"$CRATE_VERSION".tar.gz"
FILE_TO_PUBLISH_WITH_PATH="/tmp/$FILE_TO_PUBLISH"
tar czf $FILE_TO_PUBLISH_WITH_PATH $CRATE_VERSION

if [ ! -e "$FILE_TO_PUBLISH_WITH_PATH" ]; then
    echo "Error: File to upload $FILE_TO_PUBLISH_WITH_PATH does not exist."
    exit 1
fi

# Upload file to the iOS registry

REGISTRY_DEST=$CI_API_V4_URL/projects/3398/packages/generic/rust-sdk/$CRATE_VERSION/$FILE_TO_PUBLISH
echo "$REGISTRY_DEST"

RESPONSE_CODE=$(curl -s -o /dev/null -w "%{http_code}" --header "PRIVATE-TOKEN: $iOSRegistryAccessToken" --upload-file "$FILE_TO_PUBLISH_WITH_PATH" "$REGISTRY_DEST")

if [ "$RESPONSE_CODE" -ne 201 ]; then
    echo "Error: Registry request HTTP response code is "$RESPONSE_CODE"."
    exit 1
fi
