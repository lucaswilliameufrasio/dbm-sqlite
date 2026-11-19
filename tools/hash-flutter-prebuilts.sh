#!/usr/bin/env bash
# Write checksums.sha256 for the vendored Flutter binaries that exist.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
package="$root/flutter/dbm_sqlite"
cd "$package"

files=()
while IFS= read -r file; do
  files+=("$file")
done < <(
  {
    printf '%s\n' \
      android/src/main/jniLibs/arm64-v8a/libdbm_sqlite.so \
      android/src/main/jniLibs/armeabi-v7a/libdbm_sqlite.so \
      android/src/main/jniLibs/x86_64/libdbm_sqlite.so \
      android/src/main/jniLibs/arm64-v8a/libdbm_sqlite_cipher.so \
      android/src/main/jniLibs/armeabi-v7a/libdbm_sqlite_cipher.so \
      android/src/main/jniLibs/x86_64/libdbm_sqlite_cipher.so \
      linux/lib/x86_64/libdbm_sqlite_flutter.so \
      linux/lib/x86_64/libdbm_sqlite_cipher.so
    if [[ -d ios/Frameworks/DbmSqlite.xcframework ]]; then
      find ios/Frameworks/DbmSqlite.xcframework -type f | sort
    fi
  }
)

missing=0
for file in "${files[@]}"; do
  if [[ ! -f "$file" ]]; then
    echo "missing prebuilt $file" >&2
    missing=1
  fi
done
if [[ "$missing" -ne 0 ]]; then
  exit 1
fi

if command -v sha256sum >/dev/null 2>&1; then
  sha256sum "${files[@]}" > checksums.sha256
else
  shasum -a 256 "${files[@]}" > checksums.sha256
fi

echo "wrote $package/checksums.sha256"
