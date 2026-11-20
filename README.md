# dbm-sqlite

Synchronous SQLite library for application cores. It does not depend on
`database-migrator`. Version 0.1.0 opens a database file on the calling thread
and applies pending timestamped SQL. The journal records each version, the
SHA-256 of its exact SQL bytes, and whether that execution is `started`,
`applied`, or `failed`.

Authoring stays in the `dbm` binary of
[database-migrator](https://github.com/lucaswilliameufrasio/database-migrator).
This crate is not published to crates.io.

## Experimental SQLCipher

The previous `sqlcipher-src 419.0.6` was affected by RUSTSEC-2026-0502.
The lockfile now uses the patched `420.0.0`; `cargo audit` reported no
vulnerabilities on 2026-11-18. This is not a complete security review.

The optional Cargo feature `sqlcipher` adds
`Database::open_encrypted(path, migrations, key)`. The caller supplies and
protects a nonempty passphrase; the library does not store or rotate it.
Encryption is not a conversion of a plaintext database. Wrong keys and
plaintext files are rejected without a plaintext fallback. The ordinary
`Database::open` continues to open plaintext SQLite.

Cargo features unify across a dependency graph: enabling `sqlcipher` selects
the bundled SQLCipher engine for all consumers of the same `libsqlite3-sys`
in the process, including consumers that use plaintext databases. This Rust
API is experimental. The Rust-only `doctor_encrypted` inspects the journal
using a caller-supplied key and does not apply migrations. It reports a wrong
key and an invalid encrypted file alike. SQLite may maintain WAL shared-memory
sidecars; this is not a byte-preservation guarantee for those sidecars.
Flutter exposes `DbmDatabase.openEncrypted`, `doctorEncrypted`, and
`supportsSqlCipher`. They accept caller-owned UTF-8 key bytes. Linux x86_64
and Android (arm64-v8a, armeabi-v7a and x86_64) include separate SQLite and
SQLCipher libraries; select SQLCipher when
building or running the Flutter app with
`--dart-define=DBM_SQLITE_SQLCIPHER=true`. The selected library is fixed per
isolate; do not mix variants or transfer handles across differently configured
isolates. Without the flag, the default library remains SQLite. Even when
SQLCipher is selected, `open` creates plaintext files: use `openEncrypted`
for encryption. Missing SQLCipher is an error, never a plaintext fallback.
iOS includes separate SQLite and SQLCipher XCFrameworks. Select SQLCipher with
the same `--dart-define=DBM_SQLITE_SQLCIPHER=true`. Android SQLCipher was exercised on
an Android 15 x86_64 emulator and, on 2026-11-21, on an arm64-v8a Android 15
emulator. The arm64 run passed the ledger and default SQLite journeys. The
SQLCipher journey opened the encrypted file and showed the new expense on
screen; its text finder failed. armeabi-v7a was not run. On an Apple Silicon Mac, both iOS
frameworks were rebuilt and the SQLite, SQLCipher, and missing-cipher journeys passed
on an iPhone 18 Pro Max simulator running iOS 27.0. That run is recorded in
[docs/ios-validation.md](docs/ios-validation.md). The recorded run is the simulator
only: a physical device, code signing, a macOS desktop build, and production use
remain outside it. The Dart API clears its temporary
native key buffer, not the caller's bytes or other runtime/SQLCipher copies.
No key custody, rotation, encrypted backup, or
cross-platform mobile support is provided. See
[ADR-006](docs/adr/ADR-006-sqlcipher-opt-in.md).

The public connection type now uses `rusqlite 0.43`. Applications depending
directly on `rusqlite 0.41` must upgrade in coordination with their
`dbm-sqlite` dependency to avoid conflicting SQLite native libraries.
The Android, Linux x86_64, and iOS Flutter binaries were rebuilt for `0.43`.
The iOS rebuild and the simulator journeys are recorded in
[docs/ios-validation.md](docs/ios-validation.md).

## Open a file

Requires Rust 1.89 or newer. The repository toolchain is 1.99.1. SQLite is
bundled.

```rust
let migrations = [dbm_sqlite::Migration {
    version: "20261115000000000000",
    sql: "CREATE TABLE note (id INTEGER PRIMARY KEY);",
}];
let database = dbm_sqlite::Database::open(path, &migrations)?;
let connection = database.connection()?;
```

`open` creates the file when it is missing and refuses to follow a symlink.
It reads the journal before migration SQL. A checksum mismatch or an unknown
execution stops the open, and an existing file is left in place. The returned
value owns the only writable connection. A second `open` of that file fails
and does not write. A worker asks the owner with `Database::call(path, ...)`.
SQL runs on the calling thread while that call holds the connection. Shipped
applications move forward; this version has no down migration.

## Read a diagnosis

```rust
let report = dbm_sqlite::doctor(path, "1.2.3", &migrations);
let text = report.to_string();
```

`doctor` only reads. Pass the application version. The text includes that
version, the bundled SQLite version, the journal head, its checksum, and the
execution state. It leaves out the path, the SQL text, table names, and rows
from application tables. Attach `text` to a crash report. A missing file is
not created.

## Flutter

The plugin in `flutter/dbm_sqlite` vendors Android shared libraries and separate
iOS XCFrameworks for SQLite and SQLCipher. `flutter pub get` downloads those
files from this repository.
The app build does not run Cargo. SQL stays in the app and is applied on
open. Checksums are in `flutter/dbm_sqlite/checksums.sha256`. An experimental
Linux x86_64 Flutter prebuilts are included; they require glibc 2.34 or newer.
Maintainers rebuild both with `tools/vendor-linux-prebuilts.sh` inside Debian
Bookworm. Both are packaged, but only the selected variant is loaded.
On 2026-11-20 the example journeys passed on an x86_64 CachyOS host with
glibc 2.44 and Flutter 3.50.0, using `-d linux` under Xvfb: ledger (2),
default SQLite (1), and SQLCipher (1). That run is not production and does
not cover Linux ARM. Other desktop platforms, web, Expo, and React Native
are not supported by this package.
`flutter/dbm_sqlite/example` is the same expense ledger on Android, iOS, and Linux x86_64.

## Checks

```sh
cargo fmt --all -- --check
git diff --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo build --workspace --all-features --locked
```

License: MIT OR Apache-2.0.
