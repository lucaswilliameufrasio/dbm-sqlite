# dbm-sqlite

Synchronous SQLite library for application cores. It does not depend on
`database-migrator`. Version 0.1.0 opens a database file on the calling thread.
The migration journal is a later version.

Authoring stays in the `dbm` binary of
[database-migrator](https://github.com/lucaswilliameufrasio/database-migrator).
This crate is not published to crates.io.

## Open a file

Requires Rust 1.89 or newer. The repository toolchain is 1.99.1. SQLite is
bundled.

```rust
let database = dbm_sqlite::Database::open(path)?;
let connection = database.connection();
```

`open` creates the file when it is missing and refuses to follow a symlink.
SQL runs on `connection` in the calling thread.

## Checks

```sh
cargo fmt --all -- --check
git diff --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo build --workspace --all-features --locked
```

License: MIT OR Apache-2.0.
