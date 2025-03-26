#!/bin/bash

set -eu

git-cliff mail-uniffi-v0.68.2.. > mail/mail-uniffi/CHANGELOG.md
cat mail/mail-uniffi/CHANGELOG.old.md >> mail/mail-uniffi/CHANGELOG.md

