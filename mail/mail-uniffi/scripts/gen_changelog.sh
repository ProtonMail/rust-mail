#!/bin/bash

set -eu

git cliff --latest --prepend ./mail/mail-uniffi/CHANGELOG.md

