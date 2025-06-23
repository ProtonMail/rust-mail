#!/usr/bin/env bash

set -e
set -x

# Set scheme and destination
SCHEME="ProtonMail"
DEVICE_ARCH="arm64"
DESTINATION="platform=iOS Simulator,id=$DEVICE_ID,arch=$DEVICE_ARCH"
BUNDLE_ID="ch.protonmail.protonmail"

xcrun xcodebuild \
        -scheme "$SCHEME" \
        -project "$IOS_REPO_ROOT/ProtonMail.xcodeproj" \
        -configuration Debug \
        -destination "$DESTINATION" \
        -sdk iphonesimulator18.4 \
        -derivedDataPath "./target/ios-build" \
        -resolvePackageDependencies

# Build the app for the simulator
xcrun xcodebuild \
        -scheme "$SCHEME" \
        -project "$IOS_REPO_ROOT/ProtonMail.xcodeproj" \
        -configuration Debug \
        -destination "$DESTINATION" \
        -sdk iphonesimulator18.4 \
        -derivedDataPath "./target/ios-build" \
        REMOVE_STATIC_EXECUTABLES_FROM_EMBEDDED_BUNDLES=NO \
        build

# Find the path to the built app
APP_PATH=$(find ./target/ios-build -type d -name "$SCHEME.app" | sort | tail -n 1)

# Boot the simulator if not running
xcrun simctl boot "$DEVICE_ID" || true

# Install and launch the app
xcrun simctl install "$DEVICE_ID" "$APP_PATH"
xcrun simctl launch "$DEVICE_ID" "$BUNDLE_ID"

