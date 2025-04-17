#!/bin/bash

set -eu

git cliff --unreleased --tag "$1" --prepend ./mail/mail-uniffi/CHANGELOG.md



