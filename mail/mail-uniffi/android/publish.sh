#!/bin/bash

set -eo pipefail

cd ./mail/mail-uniffi/android/

# Set http proxy options to java tools
export JAVA_TOOL_OPTIONS="-Dhttp.proxyHost=$( echo ${http_proxy##http://} | cut -d':' -f1 ) -Dhttp.proxyPort=$( echo ${http_proxy##http://} | cut -d':' -f2 ) -Dhttps.proxyHost=$( echo ${https_proxy##http://} | cut -d':' -f1 ) -Dhttps.proxyPort=$( echo ${https_proxy##http://} | cut -d':' -f2 ) -Dhttp.nonProxyHosts=\"$( echo $no_proxy | tr ',' '|' )\" -Dhttp.socketTimeout=600000 -Dhttp.connectionTimeout=600000 -Dhttps.socketTimeout=600000 -Dhttps.connectionTimeout=600000"
export GRADLE_USER_HOME=`pwd`/.gradle

# Set version for maven lib release
CRATE_VERSION=$(cargo metadata --locked --format-version 1 --no-deps | jq '.packages[] | select(.name == "proton-mail-uniffi") | .version' | jq -r)
sed -i "s%    version = .*%    version = \"$CRATE_VERSION\"%g" "./lib/build.gradle.kts"

# Build and Publish lib to maven
./gradlew publishAllPublicationsToMavenCentralRepository
