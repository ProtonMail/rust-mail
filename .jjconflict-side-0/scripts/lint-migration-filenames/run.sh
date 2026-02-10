#!/bin/bash

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECK_SCRIPT="$SCRIPT_DIR/check_multiple_directories.sh"

find . -type f \( -name "*.sql" -o -name "*.rs" \) -print0 \
  | xargs -0 -n1 dirname \
  | grep -Ei 'migrations' \
  | grep -v 'online_migrations' \
  | sort -u \
  | xargs "$CHECK_SCRIPT"
