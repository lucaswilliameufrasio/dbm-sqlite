//! Synchronous SQLite for application cores.
//!
//! This crate does not depend on `database-migrator`. `Database::open` reads
//! the local journal before it writes migration SQL. An applied version keeps
//! its checksum. An unknown execution blocks writes. A failure leaves an
//! existing database file in place.

mod checksum;
mod error;
mod guard;
mod journal;
mod version;

use std::path::Path;

pub use error::{ChecksumMismatch, Error, ErrorKind, UnknownExecution};

/// One timestamped SQL migration.
///
/// `version` is a 20-digit UTC timestamp, `YYYYMMDDHHMMSSffffff`. `sql` is
/// hashed as exact bytes. Shipped applications move forward: this crate does
/// not run a down migration.
#[derive(Debug, Clone, Copy)]
pub struct Migration<'a> {
    pub version: &'a str,
    pub sql: &'a str,
}

/// An open SQLite file owned by this value.
///
/// Dropping the value closes the file. SQL runs through [`Database::connection`]
/// on the calling thread.
#[derive(Debug)]
#[must_use = "dropping the database closes the file"]
pub struct Database {
    connection: rusqlite::Connection,
}

impl Database {
    /// Opens a SQLite file and applies pending migrations before returning it.
    ///
    /// When `path` is missing, the file is created. A symlink is refused. The
    /// journal is read before migration SQL. A checksum mismatch, a missing
    /// applied migration, or an unknown execution returns an error without
    /// writing. Migration SQL that fails is rolled back; earlier applied
    /// migrations stay in the file.
    ///
    /// # Examples
    ///
    /// ```
    /// let path = std::env::temp_dir().join(format!(
    ///     "dbm-sqlite-doc-{}",
    ///     std::process::id()
    /// ));
    /// let _ = std::fs::remove_file(&path);
    /// let migrations = [dbm_sqlite::Migration {
    ///     version: "20261115000000000000",
    ///     sql: "CREATE TABLE note (id INTEGER PRIMARY KEY);",
    /// }];
    /// let database = dbm_sqlite::Database::open(&path, &migrations)?;
    /// let count: i64 = database
    ///     .connection()
    ///     .query_row("SELECT COUNT(*) FROM note", [], |row| row.get(0))
    ///     .expect("count notes");
    /// assert_eq!(count, 0);
    /// drop(database);
    /// std::fs::remove_file(&path).expect("remove doc database");
    /// # Ok::<(), dbm_sqlite::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error when the path cannot be opened, the journal does not
    /// match `migrations`, or a migration fails. The [`Error`] display includes
    /// a checksum mismatch and an unknown execution. A missing parent
    /// directory, a symlink, and a directory path fail without creating a
    /// database file.
    pub fn open(path: impl AsRef<Path>, migrations: &[Migration<'_>]) -> Result<Self, Error> {
        let path = path.as_ref();
        let connection = journal::open_database(path, migrations)?;
        Ok(Self { connection })
    }

    /// Borrows the open connection.
    ///
    /// The caller runs SQL. This crate does not provide a query builder.
    #[must_use]
    pub fn connection(&self) -> &rusqlite::Connection {
        &self.connection
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use rusqlite::OpenFlags;

    use super::{Database, ErrorKind, Migration};

    const VERSION_ONE: &str = "20261115000000000001";
    const VERSION_TWO: &str = "20261115000000000002";
    const CREATE_KEPT: &str =
        "CREATE TABLE kept (id INTEGER PRIMARY KEY); INSERT INTO kept (id) VALUES (1);";

    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock before unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "dbm-sqlite-{}-{}-{}",
                std::process::id(),
                n,
                nanos
            ));
            fs::create_dir(&path).expect("create scratch directory");
            Self { path }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn migration<'a>(version: &'a str, sql: &'a str) -> Migration<'a> {
        Migration { version, sql }
    }

    fn journal_value(path: &Path, version: &str) -> String {
        let connection = rusqlite::Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .expect("open journal");
        connection
            .query_row(
                "SELECT state || ' ' || checksum FROM dbm_sqlite_journal WHERE version = ?1",
                [version],
                |row| row.get(0),
            )
            .expect("read journal row")
    }

    fn table_names(path: &Path) -> Vec<String> {
        let connection = rusqlite::Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .expect("open tables");
        let mut statement = connection
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY name",
            )
            .expect("prepare tables");
        let mut rows = statement.query([]).expect("query tables");
        let mut names = Vec::new();
        while let Some(row) = rows.next().expect("table row") {
            names.push(row.get(0).expect("table name"));
        }
        names
    }

    #[test]
    fn open_creates_the_file_and_applies_pending_sql_once() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        let migrations = [migration(
            VERSION_ONE,
            "CREATE TABLE note (id INTEGER PRIMARY KEY); INSERT INTO note (id) VALUES (1);",
        )];

        assert!(!path.exists());
        let database = Database::open(&path, &migrations).expect("create database");
        let inserted: i64 = database
            .connection()
            .query_row("SELECT COUNT(*) FROM note", [], |row| row.get(0))
            .expect("count notes");
        assert_eq!(inserted, 1);
        drop(database);

        let reopened = Database::open(&path, &migrations).expect("reopen database");
        let inserted: i64 = reopened
            .connection()
            .query_row("SELECT COUNT(*) FROM note", [], |row| row.get(0))
            .expect("count notes again");
        assert_eq!(inserted, 1);
        assert!(journal_value(&path, VERSION_ONE).starts_with("applied "));
        assert!(path.is_file());
        assert!(fs::metadata(&path).expect("stat database").len() > 0);
    }

    #[test]
    fn migrations_run_in_version_order() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        let migrations = [
            migration(VERSION_TWO, "INSERT INTO note (id) VALUES (2);"),
            migration(VERSION_ONE, "CREATE TABLE note (id INTEGER PRIMARY KEY);"),
        ];

        let database = Database::open(&path, &migrations).expect("apply in order");
        let id: i64 = database
            .connection()
            .query_row("SELECT id FROM note", [], |row| row.get(0))
            .expect("read note");
        assert_eq!(id, 2);
    }

    #[test]
    fn checksum_mismatch_is_visible_and_preserves_the_file() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        let original =
            "CREATE TABLE note (id INTEGER PRIMARY KEY); INSERT INTO note (id) VALUES (7);";
        let migrations = [migration(VERSION_ONE, original)];
        drop(Database::open(&path, &migrations).expect("apply original"));
        let before = fs::read(&path).expect("read original file");
        let applied = journal_value(&path, VERSION_ONE);

        let edited = [migration(
            VERSION_ONE,
            "CREATE TABLE note (id INTEGER PRIMARY KEY); INSERT INTO note (id) VALUES (7); -- edited",
        )];
        let error = Database::open(&path, &edited).expect_err("checksum must mismatch");
        let mismatch = error
            .checksum_mismatch()
            .expect("visible checksum mismatch");
        assert_eq!(mismatch.version, VERSION_ONE);
        assert_eq!(format!("applied {}", mismatch.applied_checksum), applied);
        assert_ne!(mismatch.applied_checksum, mismatch.provided_checksum);
        assert!(error.to_string().contains("checksum mismatch"));
        assert!(error.to_string().contains(mismatch.applied_checksum));
        assert!(error.to_string().contains(mismatch.provided_checksum));
        assert_eq!(fs::read(&path).expect("read preserved file"), before);
        assert_eq!(journal_value(&path, VERSION_ONE), applied);
    }

    #[test]
    fn unknown_execution_blocks_writes_and_preserves_the_file() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        let migrations = [migration(
            VERSION_ONE,
            "CREATE TABLE note (id INTEGER PRIMARY KEY); INSERT INTO note (id) VALUES (1);",
        )];
        let database = Database::open(&path, &migrations).expect("apply");
        database
            .connection()
            .execute(
                "UPDATE dbm_sqlite_journal SET state = 'started' WHERE version = ?1",
                [VERSION_ONE],
            )
            .expect("mark started");
        drop(database);
        let before = fs::read(&path).expect("read started file");

        let error = Database::open(&path, &migrations).expect_err("unknown execution");
        let unknown = error
            .unknown_execution()
            .expect("visible unknown execution");
        assert_eq!(unknown.version, VERSION_ONE);
        assert_eq!(unknown.state, "started");
        assert!(error.to_string().contains("unknown execution"));
        assert!(error.to_string().contains("writes are blocked"));
        assert_eq!(fs::read(&path).expect("read preserved file"), before);

        let connection = rusqlite::Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .expect("read note");
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM note", [], |row| row.get(0))
            .expect("count notes");
        assert_eq!(count, 1);
    }

    #[test]
    fn failed_migration_preserves_earlier_rows_and_can_be_retried() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        let first = [migration(VERSION_ONE, CREATE_KEPT)];
        drop(Database::open(&path, &first).expect("apply first"));

        let broken = [
            migration(VERSION_ONE, CREATE_KEPT),
            migration(
                VERSION_TWO,
                "CREATE TABLE added (id INTEGER PRIMARY KEY); INSERT INTO missing (id) VALUES (1);",
            ),
        ];
        let error = Database::open(&path, &broken).expect_err("second migration fails");
        match error.kind() {
            ErrorKind::Migration { version, .. } => {
                assert_eq!(version, VERSION_TWO);
            }
            other => panic!("expected migration failure, got {other:?}"),
        }
        assert!(path.is_file());
        assert!(!table_names(&path).iter().any(|name| name == "added"));
        assert!(journal_value(&path, VERSION_TWO).starts_with("failed "));
        let connection = rusqlite::Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .expect("read kept");
        let id: i64 = connection
            .query_row("SELECT id FROM kept", [], |row| row.get(0))
            .expect("kept row");
        assert_eq!(id, 1);
        drop(connection);

        let fixed = [
            migration(VERSION_ONE, CREATE_KEPT),
            migration(
                VERSION_TWO,
                "CREATE TABLE added (id INTEGER PRIMARY KEY); INSERT INTO added (id) VALUES (2);",
            ),
        ];
        let database = Database::open(&path, &fixed).expect("retry failed migration");
        let id: i64 = database
            .connection()
            .query_row("SELECT id FROM added", [], |row| row.get(0))
            .expect("added row");
        assert_eq!(id, 2);
        assert!(journal_value(&path, VERSION_TWO).starts_with("applied "));
    }

    #[test]
    fn a_failed_first_migration_keeps_the_generated_file_without_its_table() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        let migrations = [migration(
            VERSION_ONE,
            "INSERT INTO missing (id) VALUES (1);",
        )];

        let error = Database::open(&path, &migrations).expect_err("first migration fails");
        assert!(matches!(error.kind(), ErrorKind::Migration { .. }));
        assert!(path.is_file());
        assert!(!table_names(&path).iter().any(|name| name == "missing"));
        assert!(journal_value(&path, VERSION_ONE).starts_with("failed "));
    }

    #[test]
    fn missing_applied_migration_preserves_the_file() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        let migrations = [migration(VERSION_ONE, CREATE_KEPT)];
        drop(Database::open(&path, &migrations).expect("apply"));
        let before = fs::read(&path).expect("read applied file");

        let error = Database::open(&path, &[]).expect_err("missing migration");
        assert!(matches!(
            error.kind(),
            ErrorKind::MissingMigration { version } if version == VERSION_ONE
        ));
        assert_eq!(fs::read(&path).expect("read preserved file"), before);
    }

    #[test]
    fn unrecognized_file_and_non_database_bytes_stay_in_place() {
        let scratch = Scratch::new();
        let foreign = scratch.path.join("foreign.sqlite");
        let connection = rusqlite::Connection::open(&foreign).expect("create foreign");
        connection
            .execute_batch("CREATE TABLE legacy (id INTEGER PRIMARY KEY);")
            .expect("create legacy");
        drop(connection);
        let before = fs::read(&foreign).expect("read foreign");
        let error = Database::open(&foreign, &[migration(VERSION_ONE, CREATE_KEPT)])
            .expect_err("foreign file");
        assert!(matches!(error.kind(), ErrorKind::UnrecognizedFile));
        assert_eq!(fs::read(&foreign).expect("read preserved foreign"), before);

        let text = scratch.path.join("notes.txt");
        fs::write(&text, b"not a database").expect("write text");
        let error =
            Database::open(&text, &[migration(VERSION_ONE, CREATE_KEPT)]).expect_err("text file");
        assert!(matches!(error.kind(), ErrorKind::Open(_)));
        assert_eq!(
            fs::read(&text).expect("read preserved text"),
            b"not a database"
        );
    }

    #[test]
    fn invalid_input_does_not_generate_a_file() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");

        let invalid = Database::open(&path, &[migration("20260230000000000000", CREATE_KEPT)])
            .expect_err("invalid day");
        assert!(matches!(invalid.kind(), ErrorKind::InvalidVersion { .. }));
        assert!(!path.exists());

        let duplicated = [
            migration(VERSION_ONE, "CREATE TABLE a (id INTEGER);"),
            migration(VERSION_ONE, "CREATE TABLE b (id INTEGER);"),
        ];
        let duplicate = Database::open(&path, &duplicated).expect_err("duplicate");
        assert!(matches!(
            duplicate.kind(),
            ErrorKind::DuplicateVersion { .. }
        ));
        assert!(!path.exists());

        let unsafe_sql =
            Database::open(&path, &[migration(VERSION_ONE, "COMMIT;")]).expect_err("commit");
        assert!(matches!(unsafe_sql.kind(), ErrorKind::UnsafeSql { .. }));
        assert!(!path.exists());
    }

    #[test]
    fn open_refuses_a_missing_parent_directory() {
        let scratch = Scratch::new();
        let path = scratch.path.join("missing").join("app.sqlite");

        let opened = Database::open(&path, &[]);
        assert!(opened.is_err());
        assert!(!scratch.path.join("missing").exists());
    }

    #[test]
    fn open_refuses_a_directory() {
        let scratch = Scratch::new();
        let path = scratch.path.join("not-a-file");
        fs::create_dir(&path).expect("create directory");

        let opened = Database::open(&path, &[]).expect_err("directory opens");
        assert_eq!(opened.path(), path.as_path());
        assert!(matches!(opened.kind(), ErrorKind::Directory));
    }

    #[cfg(unix)]
    #[test]
    fn open_refuses_a_symlink() {
        let scratch = Scratch::new();
        let target = scratch.path.join("target.sqlite");
        let link = scratch.path.join("link.sqlite");
        fs::write(&target, b"").expect("write empty target");
        std::os::unix::fs::symlink(&target, &link).expect("create symlink");

        let opened = Database::open(&link, &[]).expect_err("symlink opens");
        assert_eq!(opened.path(), link.as_path());
        assert!(matches!(opened.kind(), ErrorKind::Symlink));
        let bytes = fs::read(&target).expect("read target");
        assert_eq!(bytes, b"");

        let database = Database::open(&target, &[]).expect("open target path");
        let count: i64 = database
            .connection()
            .query_row("SELECT COUNT(*) FROM dbm_sqlite_journal", [], |row| {
                row.get(0)
            })
            .expect("count journal");
        assert_eq!(count, 0);
    }
}
