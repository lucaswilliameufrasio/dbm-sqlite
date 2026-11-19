#!/usr/bin/env bash
# Confirm the Flutter package vendors binaries and does not compile Rust.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
package="$root/flutter/dbm_sqlite"
cd "$package"

if [[ -e hook ]]; then
  echo "flutter build must not run a native build hook" >&2
  exit 1
fi

if grep -R -I -n -E 'cargo|rustc|cargo-ndk|externalNativeBuild|CMakeLists' android ios >/dev/null; then
  echo "android or ios build files compile native code" >&2
  grep -R -I -n -E 'cargo|rustc|cargo-ndk|externalNativeBuild|CMakeLists' android ios >&2
  exit 1
fi

if grep -n -E 'cargo|rustc|add_subdirectory|execute_process|ExternalProject' linux/CMakeLists.txt >/dev/null; then
  echo "Linux packaging must not build native code" >&2
  exit 1
fi

python3 - <<'PY'
from pathlib import Path
text = Path("pubspec.yaml").read_text()
start = text.find("platforms:")
if start < 0:
    raise SystemExit("pubspec has no platforms")
block = text[start:]
for forbidden in ("macos:", "windows:", "web:"):
    if forbidden in block:
        raise SystemExit(f"pubspec platform is out of the matrix: {forbidden}")
for required in ("android:", "ios:", "linux:"):
    if required not in block:
        raise SystemExit(f"pubspec is missing {required}")
PY

required=(
  android/src/main/jniLibs/arm64-v8a/libdbm_sqlite.so
  android/src/main/jniLibs/armeabi-v7a/libdbm_sqlite.so
  android/src/main/jniLibs/x86_64/libdbm_sqlite.so
  android/src/main/jniLibs/arm64-v8a/libdbm_sqlite_cipher.so
  android/src/main/jniLibs/armeabi-v7a/libdbm_sqlite_cipher.so
  android/src/main/jniLibs/x86_64/libdbm_sqlite_cipher.so
  linux/lib/x86_64/libdbm_sqlite_flutter.so
  linux/lib/x86_64/libdbm_sqlite_cipher.so
)
for file in "${required[@]}"; do
  if [[ ! -f "$file" ]]; then
    echo "missing $file" >&2
    exit 1
  fi
done

if ! grep -q 'ios/Frameworks/DbmSqlite.xcframework/' checksums.sha256; then
  if [[ "${DBM_SQLITE_IOS_PENDING:-}" == "1" ]]; then
    echo "iOS XCFramework is not vendored yet"
  else
    echo "checksums.sha256 has no iOS XCFramework" >&2
    exit 1
  fi
fi

if command -v sha256sum >/dev/null 2>&1; then
  sha256sum -c checksums.sha256
else
  shasum -a 256 -c checksums.sha256
fi

check_elf() {
  local file="$1"
  local machine="$2"
  local header
  header=$(readelf -h "$file")
  if ! grep -q 'Type:[[:space:]]*DYN' <<<"$header"; then
    echo "$file is not a shared library" >&2
    exit 1
  fi
  if ! grep -q "Machine:.*$machine" <<<"$header"; then
    echo "$file is not $machine" >&2
    echo "$header" >&2
    exit 1
  fi
}

check_elf android/src/main/jniLibs/arm64-v8a/libdbm_sqlite.so 'AArch64'
check_elf android/src/main/jniLibs/armeabi-v7a/libdbm_sqlite.so 'ARM'
check_elf android/src/main/jniLibs/x86_64/libdbm_sqlite.so 'X86-64'
check_elf android/src/main/jniLibs/arm64-v8a/libdbm_sqlite_cipher.so 'AArch64'
check_elf android/src/main/jniLibs/armeabi-v7a/libdbm_sqlite_cipher.so 'ARM'
check_elf android/src/main/jniLibs/x86_64/libdbm_sqlite_cipher.so 'X86-64'
check_elf linux/lib/x86_64/libdbm_sqlite_flutter.so 'X86-64'
check_elf linux/lib/x86_64/libdbm_sqlite_cipher.so 'X86-64'

plain_symbols=$(nm -D --defined-only linux/lib/x86_64/libdbm_sqlite_flutter.so)
cipher_symbols=$(nm -D --defined-only linux/lib/x86_64/libdbm_sqlite_cipher.so)
for symbol in dbm_sqlite_open_encrypted dbm_sqlite_doctor_encrypted; do
  if grep -q -w "$symbol" <<<"$plain_symbols"; then
    echo "plaintext binary unexpectedly exposes $symbol" >&2
    exit 1
  fi
  if ! grep -q -w "$symbol" <<<"$cipher_symbols"; then
    echo "SQLCipher binary is missing $symbol" >&2
    exit 1
  fi
done

for abi in arm64-v8a armeabi-v7a x86_64; do
  plain_symbols=$(nm -D --defined-only "android/src/main/jniLibs/$abi/libdbm_sqlite.so")
  cipher_symbols=$(nm -D --defined-only "android/src/main/jniLibs/$abi/libdbm_sqlite_cipher.so")
  for symbol in dbm_sqlite_open_encrypted dbm_sqlite_doctor_encrypted; do
    if grep -q -w "$symbol" <<<"$plain_symbols"; then
      echo "Android $abi plaintext binary unexpectedly exposes $symbol" >&2
      exit 1
    fi
    if ! grep -q -w "$symbol" <<<"$cipher_symbols"; then
      echo "Android $abi SQLCipher binary is missing $symbol" >&2
      exit 1
    fi
  done
done

check_page() {
  local file="$1"
  local align number
  align=$(readelf -W -l "$file" | awk '/^  LOAD/ { print $NF }' | sort -u)
  if [[ -z "$align" ]]; then
    echo "$file has no LOAD segment" >&2
    exit 1
  fi
  for value in $align; do
    number=$((value))
    if [[ "$number" -lt 16384 ]]; then
      echo "$file LOAD alignment $value is below 16KB" >&2
      exit 1
    fi
  done
}

check_page android/src/main/jniLibs/arm64-v8a/libdbm_sqlite.so
check_page android/src/main/jniLibs/x86_64/libdbm_sqlite.so
check_page android/src/main/jniLibs/arm64-v8a/libdbm_sqlite_cipher.so
check_page android/src/main/jniLibs/x86_64/libdbm_sqlite_cipher.so

python3 "$root/tools/test-linux-bundle.py"

echo "flutter plugin prebuilts match checksums.sha256"
