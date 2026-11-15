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
let connection = database.connection();
```

`open` creates the file when it is missing and refuses to follow a symlink.
It reads the journal before migration SQL. A checksum mismatch or an unknown
execution stops the open, and an existing file is left in place. SQL after
that runs on `connection` in the calling thread. Shipped applications move
forward; this version has no down migration.

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

## Checks

```sh
cargo fmt --all -- --check
git diff --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo build --workspace --all-features --locked
```

License: MIT OR Apache-2.0.
