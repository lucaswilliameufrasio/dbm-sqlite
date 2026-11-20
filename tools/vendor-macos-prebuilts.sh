#!/usr/bin/env bash
# Maintainer build on macOS arm64. Flutter apps do not run this script.
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
  echo "macOS arm64 is required" >&2
  exit 1
fi

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

rustc_version=$(rustc --version)
case "$rustc_version" in
  *1.99.1*) ;;
  *)
    echo "rustc must be 1.99.1, got $rustc_version" >&2
    exit 1
    ;;
esac

sdk=$(xcrun --sdk macosx --show-sdk-path)
target_dir="$root/target/macos-flutter"
stage="$root/target/macos-xcframework"
header="$root/crates/dbm-sqlite-flutter/include/dbm_sqlite.h"
dylib_rel="aarch64-apple-darwin/flutter/libdbm_sqlite_flutter.dylib"

build_variant() {
  local variant="$1"
  if [[ "$variant" == "plain" ]]; then
    MACOSX_DEPLOYMENT_TARGET=12.0 SDKROOT="$sdk" CARGO_TARGET_DIR="$target_dir" \
      cargo build --locked --profile flutter -p dbm-sqlite-flutter --target aarch64-apple-darwin
  elif [[ "$variant" == "sqlcipher" ]]; then
    MACOSX_DEPLOYMENT_TARGET=12.0 SDKROOT="$sdk" CARGO_TARGET_DIR="$target_dir" \
      cargo build --locked --profile flutter -p dbm-sqlite-flutter --target aarch64-apple-darwin --features sqlcipher
  else
    echo "unknown variant $variant" >&2
    exit 1
  fi
}

prepare_binary() {
  local src="$1"
  local dest="$2"
  local name="$3"
  cp "$src" "$dest"
  chmod 755 "$dest"
  install_name_tool -id "@rpath/$name.framework/$name" "$dest"
  local rpath
  while IFS= read -r rpath; do
    case "$rpath" in
      @*) ;;
      *)
        install_name_tool -delete_rpath "$rpath" "$dest"
        ;;
    esac
  done < <(otool -l "$dest" | awk '$1 == "cmd" && $2 == "LC_RPATH" { show = 1; next } show && $1 == "path" { print $2; show = 0 }')
  local line
  while IFS= read -r line; do
    case "$line" in
      *$'\t'/usr/lib/*|*$'\t'@rpath/*) ;;
      *)
        echo "unexpected dependency in $name: $line" >&2
        exit 1
        ;;
    esac
  done < <(otool -L "$dest" | tail -n +2)
  codesign --force --sign - "$dest" >/dev/null
}

make_framework() {
  local src="$1"
  local name="$2"
  local bundle_id="$3"
  local fw="$stage/$name.framework"
  local version="$fw/Versions/A"
  rm -rf "$fw"
  mkdir -p "$version/Resources" "$version/Headers" "$version/Modules"
  cp "$src" "$version/$name"
  chmod 755 "$version/$name"
  install_name_tool -id "@rpath/$name.framework/Versions/A/$name" "$version/$name"
  cp "$header" "$version/Headers/dbm_sqlite.h"
  cat > "$version/Modules/module.modulemap" <<MOD
framework module $name {
    header "dbm_sqlite.h"
    export *
}
MOD
  cat > "$version/Resources/Info.plist" <<PLIST
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
  <string>12.0</string>
  <key>CFBundleSupportedPlatforms</key>
  <array>
    <string>MacOSX</string>
  </array>
</dict>
</plist>
PLIST
  ln -s A "$fw/Versions/Current"
  ln -s "Versions/Current/$name" "$fw/$name"
  ln -s Versions/Current/Resources "$fw/Resources"
  ln -s Versions/Current/Headers "$fw/Headers"
  ln -s Versions/Current/Modules "$fw/Modules"
  codesign --force --sign - "$version/$name" >/dev/null
}

write_xcframework() {
  local name="$1"
  rm -rf "$stage/$name.xcframework"
  xcodebuild -create-xcframework \
    -framework "$stage/$name.framework" \
    -output "$stage/$name.xcframework"
}

rm -rf "$stage"
mkdir -p "$stage"

build_variant plain
prepare_binary "$target_dir/$dylib_rel" "$stage/plain.dylib" DbmSqlite
build_variant sqlcipher
prepare_binary "$target_dir/$dylib_rel" "$stage/cipher.dylib" DbmSqliteCipher

make_framework "$stage/plain.dylib" DbmSqlite io.github.lucaswilliameufrasio.dbm-sqlite
make_framework "$stage/cipher.dylib" DbmSqliteCipher io.github.lucaswilliameufrasio.dbm-sqlite-cipher
write_xcframework DbmSqlite
write_xcframework DbmSqliteCipher

destination="$root/flutter/dbm_sqlite/macos/Frameworks"
mkdir -p "$destination"
rm -rf "$destination/DbmSqlite.xcframework" "$destination/DbmSqliteCipher.xcframework"
cp -R "$stage/DbmSqlite.xcframework" "$destination/DbmSqlite.xcframework"
cp -R "$stage/DbmSqliteCipher.xcframework" "$destination/DbmSqliteCipher.xcframework"
bash "$root/tools/hash-flutter-prebuilts.sh"
echo "wrote $destination"
