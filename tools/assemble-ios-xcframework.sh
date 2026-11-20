#!/usr/bin/env bash
# Maintainer build on macOS. Flutter apps do not run this script.
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "iOS XCFramework assembly needs macOS and Xcode." >&2
  exit 1
fi

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

variant="${1:-plain}"
case "$variant" in
  plain)
    name=DbmSqlite
    bundle_id=io.github.lucaswilliameufrasio.dbm-sqlite
    cargo_features=()
    stage="$root/target/ios-xcframework"
    ;;
  sqlcipher)
    name=DbmSqliteCipher
    bundle_id=io.github.lucaswilliameufrasio.dbm-sqlite-cipher
    cargo_features=(--features sqlcipher)
    stage="$root/target/ios-xcframework-cipher"
    ;;
  *)
    echo "Usage: bash tools/assemble-ios-xcframework.sh [plain|sqlcipher]" >&2
    exit 1
    ;;
esac

rustup target add aarch64-apple-ios aarch64-apple-ios-sim --toolchain 1.99.1

device_sdk=$(xcrun --sdk iphoneos --show-sdk-path)
sim_sdk=$(xcrun --sdk iphonesimulator --show-sdk-path)

# Device stays on iOS 13. The arm64 simulator does not exist before iOS 14,
# and rustc raises that slice to 14 even when the deployment target is lower.
# macOS /bin/bash is 3.2: an empty array under set -u is an unbound variable.
build_flutter() {
  local target="$1"
  local sdk="$2"
  local deployment="$3"
  if [ "${#cargo_features[@]}" -eq 0 ]; then
    IPHONEOS_DEPLOYMENT_TARGET="$deployment" SDKROOT="$sdk" \
      cargo build --locked --profile flutter -p dbm-sqlite-flutter --target "$target"
  else
    IPHONEOS_DEPLOYMENT_TARGET="$deployment" SDKROOT="$sdk" \
      cargo build --locked --profile flutter -p dbm-sqlite-flutter --target "$target" "${cargo_features[@]}"
  fi
}

build_flutter aarch64-apple-ios "$device_sdk" 13.0
build_flutter aarch64-apple-ios-sim "$sim_sdk" 14.0

rm -rf "$stage"
mkdir -p "$stage/include"
cp "$root/crates/dbm-sqlite-flutter/include/dbm_sqlite.h" "$stage/include/dbm_sqlite.h"

make_framework() {
  local src_dylib="$1"
  local dest_dir="$2"
  local platform="$3"
  local minimum_os="$4"
  local fw="$dest_dir/$name.framework"
  mkdir -p "$fw/Headers" "$fw/Modules"
  cp "$src_dylib" "$fw/$name"
  install_name_tool -id "@rpath/$name.framework/$name" "$fw/$name"
  chmod 755 "$fw/$name"
  cp "$stage/include/dbm_sqlite.h" "$fw/Headers/dbm_sqlite.h"
  cat > "$fw/Modules/module.modulemap" <<EOF
framework module $name {
    header "dbm_sqlite.h"
    export *
}
EOF
  cat > "$fw/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>${name}</string>
  <key>CFBundleIdentifier</key>
  <string>${bundle_id}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>${name}</string>
  <key>CFBundlePackageType</key>
  <string>FMWK</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>0.1.0</string>
  <key>MinimumOSVersion</key>
  <string>${minimum_os}</string>
  <key>CFBundleSupportedPlatforms</key>
  <array>
    <string>${platform}</string>
  </array>
</dict>
</plist>
EOF
}

make_framework \
  "$root/target/aarch64-apple-ios/flutter/libdbm_sqlite_flutter.dylib" \
  "$stage/device" \
  "iPhoneOS" \
  "13.0"
make_framework \
  "$root/target/aarch64-apple-ios-sim/flutter/libdbm_sqlite_flutter.dylib" \
  "$stage/simulator" \
  "iPhoneSimulator" \
  "14.0"

xcodebuild -create-xcframework \
  -framework "$stage/device/$name.framework" \
  -framework "$stage/simulator/$name.framework" \
  -output "$stage/$name.xcframework"

echo "wrote $stage/$name.xcframework"
