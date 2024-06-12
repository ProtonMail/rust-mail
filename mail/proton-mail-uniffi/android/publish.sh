#!/bin/bash

set -eo pipefail

cd proton-mail-uniffi/android/

# Set http proxy options to java tools
export JAVA_TOOL_OPTIONS="-Dhttp.proxyHost=$( echo ${http_proxy##http://} | cut -d':' -f1 ) -Dhttp.proxyPort=$( echo ${http_proxy##http://} | cut -d':' -f2 ) -Dhttps.proxyHost=$( echo ${https_proxy##http://} | cut -d':' -f1 ) -Dhttps.proxyPort=$( echo ${https_proxy##http://} | cut -d':' -f2 ) -Dhttp.nonProxyHosts=\"$( echo $no_proxy | tr ',' '|' )\" -Dhttp.socketTimeout=600000 -Dhttp.connectionTimeout=600000 -Dhttps.socketTimeout=600000 -Dhttps.connectionTimeout=600000"
export GRADLE_USER_HOME=`pwd`/.gradle

# Set version for maven lib release
cargo generate-lockfile
CRATE_VERSION=$(cargo pkgid -p proton-mail-uniffi | cut -d "#" -f2)
sed -i "s%    version = .*%    version = \"$CRATE_VERSION\"%g" "./lib/build.gradle.kts"

# Build and Publish lib to maven
./gradlew publishAllPublicationsToMavenCentralRepository
