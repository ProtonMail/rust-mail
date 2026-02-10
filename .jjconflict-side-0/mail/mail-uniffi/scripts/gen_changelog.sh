#!/bin/bash

set -euo pipefail

uv run --project ./scripts/changelog changelog "$@" \
	--only mail-uniffi \
	--init mail-uniffi-v0.68.2 \
	> mail/mail-uniffi/CHANGELOG.md

cat mail/mail-uniffi/CHANGELOG.old.md >> mail/mail-uniffi/CHANGELOG.md
