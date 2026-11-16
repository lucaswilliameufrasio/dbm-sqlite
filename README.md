# dbm-sqlite

Synchronous SQLite library for application cores. It does not depend on
`database-migrator`. Version 0.1.0 opens a database file on the calling thread
and applies pending timestamped SQL. The journal records each version, the
SHA-256 of its exact SQL bytes, and whether that execution is `started`,
`applied`, or `failed`.

Authoring stays in the `dbm` binary of
[database-migrator](https://github.com/lucaswilliameufrasio/database-migrator).
This crate is not published to crates.io.

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

The plugin in `flutter/dbm_sqlite` vendors Android shared libraries and an iOS
XCFramework. `flutter pub get` downloads those files from this repository.
The app build does not run Cargo. SQL stays in the app and is applied on
open. Checksums are in `flutter/dbm_sqlite/checksums.sha256`. Desktop, web,
Expo, and React Native are outside this package.
`flutter/dbm_sqlite/example` is the same expense ledger on Android and iOS.

## Checks

```sh
cargo fmt --all -- --check
git diff --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo build --workspace --all-features --locked
```

License: MIT OR Apache-2.0.
