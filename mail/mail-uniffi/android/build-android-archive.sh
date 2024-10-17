#!/bin/bash

set -eo pipefail

# Set http proxy options to java tools
if [[ -n $http_proxy ]]; then

  echo "Exporting java options with http proxy..."

  export JAVA_TOOL_OPTIONS="-Dhttp.proxyHost=$( echo ${http_proxy##http://} | cut -d':' -f1 ) -Dhttp.proxyPort=$( echo ${http_proxy##http://} | cut -d':' -f2 ) -Dhttps.proxyHost=$( echo ${https_proxy##http://} | cut -d':' -f1 ) -Dhttps.proxyPort=$( echo ${https_proxy##http://} | cut -d':' -f2 ) -Dhttp.nonProxyHosts=\"$( echo $no_proxy | tr ',' '|' )\" -Dhttp.socketTimeout=600000 -Dhttp.connectionTimeout=600000 -Dhttps.socketTimeout=600000 -Dhttps.connectionTimeout=600000"

fi

export GRADLE_USER_HOME=`pwd`/.gradle

# Build and Publish lib to maven
cd ./mail/mail-uniffi/android/
./gradlew :lib:assembleRelease
