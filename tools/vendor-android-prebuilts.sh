#!/usr/bin/env bash
# Maintainer build. Flutter apps do not run this script.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

ndk="${ANDROID_NDK_HOME:-$HOME/Android/Sdk/ndk/27.2.12479018}"
if [[ ! -d "$ndk" ]]; then
  echo "Android NDK not found at $ndk" >&2
  exit 1
fi
export ANDROID_NDK_HOME="$ndk"

rustup target add \
  aarch64-linux-android \
  armv7-linux-androideabi \
  x86_64-linux-android \
  --toolchain 1.99.1

page='-C link-arg=-Wl,-z,max-page-size=16384'
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="$page"
export CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_RUSTFLAGS="$page"
export CARGO_TARGET_X86_64_LINUX_ANDROID_RUSTFLAGS="$page"

plain="$root/target/android-plain-prebuilts"
cipher="$root/target/android-cipher-prebuilts"
CARGO_TARGET_DIR="$root/target/android-plain" cargo ndk \
  --target arm64-v8a \
  --target armeabi-v7a \
  --target x86_64 \
  -o "$plain" \
  build --locked --profile flutter -p dbm-sqlite-flutter

CARGO_TARGET_DIR="$root/target/android-cipher" cargo ndk \
  --target arm64-v8a \
  --target armeabi-v7a \
  --target x86_64 \
  -o "$cipher" \
  build --locked --profile flutter -p dbm-sqlite-flutter --features sqlcipher

out="$root/flutter/dbm_sqlite/android/src/main/jniLibs"

for abi in arm64-v8a armeabi-v7a x86_64; do
  for variant in "$plain" "$cipher"; do
    if [[ ! -f "$variant/$abi/libdbm_sqlite_flutter.so" ]]; then
      echo "cargo-ndk did not write a library for $abi" >&2
      exit 1
    fi
  done
done
for abi in arm64-v8a armeabi-v7a x86_64; do
  install -m 755 "$plain/$abi/libdbm_sqlite_flutter.so" "$out/$abi/libdbm_sqlite.so"
  install -m 755 "$cipher/$abi/libdbm_sqlite_flutter.so" "$out/$abi/libdbm_sqlite_cipher.so"
done

bash "$root/tools/hash-flutter-prebuilts.sh"
