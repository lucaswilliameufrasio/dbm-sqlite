#!/usr/bin/env bash
# Maintainer build on macOS. Flutter apps do not run this script.
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "iOS XCFramework assembly needs macOS and Xcode." >&2
  exit 1
fi

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

rustup target add aarch64-apple-ios aarch64-apple-ios-sim --toolchain 1.99.1

device_sdk=$(xcrun --sdk iphoneos --show-sdk-path)
sim_sdk=$(xcrun --sdk iphonesimulator --show-sdk-path)

# Device stays on iOS 13. The arm64 simulator does not exist before iOS 14,
# and rustc raises that slice to 14 even when the deployment target is lower.
IPHONEOS_DEPLOYMENT_TARGET=13.0 SDKROOT="$device_sdk" cargo build --locked --profile flutter -p dbm-sqlite-flutter --target aarch64-apple-ios
IPHONEOS_DEPLOYMENT_TARGET=14.0 SDKROOT="$sim_sdk" cargo build --locked --profile flutter -p dbm-sqlite-flutter --target aarch64-apple-ios-sim

stage="$root/target/ios-xcframework"
rm -rf "$stage"
mkdir -p "$stage/include"
cp "$root/crates/dbm-sqlite-flutter/include/dbm_sqlite.h" "$stage/include/dbm_sqlite.h"

make_framework() {
  local src_dylib="$1"
  local dest_dir="$2"
  local platform="$3"
  local minimum_os="$4"
  local fw="$dest_dir/DbmSqlite.framework"
  mkdir -p "$fw/Headers" "$fw/Modules"
  cp "$src_dylib" "$fw/DbmSqlite"
  install_name_tool -id "@rpath/DbmSqlite.framework/DbmSqlite" "$fw/DbmSqlite"
  chmod 755 "$fw/DbmSqlite"
  cp "$stage/include/dbm_sqlite.h" "$fw/Headers/dbm_sqlite.h"
  cat > "$fw/Modules/module.modulemap" <<'EOF'
framework module DbmSqlite {
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
  <string>DbmSqlite</string>
  <key>CFBundleIdentifier</key>
  <string>io.github.lucaswilliameufrasio.dbm-sqlite</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>DbmSqlite</string>
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

rm -rf "$stage/DbmSqlite.xcframework"
xcodebuild -create-xcframework \
  -framework "$stage/device/DbmSqlite.framework" \
  -framework "$stage/simulator/DbmSqlite.framework" \
  -output "$stage/DbmSqlite.xcframework"

echo "wrote $stage/DbmSqlite.xcframework"
