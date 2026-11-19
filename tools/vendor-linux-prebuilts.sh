#!/usr/bin/env bash
# Maintainer build inside Debian Bookworm x86_64; app builds do not run Cargo.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

if [[ "$(uname -s)" != "Linux" || "$(uname -m)" != "x86_64" ]]; then
  echo "Linux x86_64 is required" >&2
  exit 1
fi
if [[ "$(getconf GNU_LIBC_VERSION)" != "glibc 2.36" ]]; then
  echo "Build in Debian Bookworm with glibc 2.36" >&2
  exit 1
fi

CARGO_TARGET_DIR="$root/target/desktop-bookworm" \
  cargo build --locked --profile flutter -p dbm-sqlite-flutter
CARGO_TARGET_DIR="$root/target/desktop-bookworm-sqlcipher" \
  cargo build --locked --profile flutter -p dbm-sqlite-flutter --features sqlcipher

destination="$root/flutter/dbm_sqlite/linux/lib/x86_64"
mkdir -p "$destination"
install -m 755 "$root/target/desktop-bookworm/flutter/libdbm_sqlite_flutter.so" \
  "$destination/libdbm_sqlite_flutter.so"
install -m 755 "$root/target/desktop-bookworm-sqlcipher/flutter/libdbm_sqlite_flutter.so" \
  "$destination/libdbm_sqlite_cipher.so"
bash "$root/tools/hash-flutter-prebuilts.sh"
