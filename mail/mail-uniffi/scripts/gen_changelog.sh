#!/bin/bash

set -eu

uv run --project ./scripts/changelog changelog --after mail-uniffi-v0.68.2 > mail/mail-uniffi/CHANGELOG.md

cat mail/mail-uniffi/CHANGELOG.old.md >> mail/mail-uniffi/CHANGELOG.md
