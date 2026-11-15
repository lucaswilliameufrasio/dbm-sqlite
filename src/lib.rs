//! Synchronous SQLite for application cores.
//!
//! This crate does not depend on `database-migrator`. Version 0.1.0 opens a
//! database file on the calling thread. Migration journal checks are a later
//! version.

mod error;

use std::path::Path;

use rusqlite::{Connection, OpenFlags};

pub use error::Error;

/// An open SQLite file owned by this value.
///
/// Dropping the value closes the file. SQL runs through [`Database::connection`]
/// on the calling thread.
#[derive(Debug)]
#[must_use = "dropping the database closes the file"]
pub struct Database {
    connection: Connection,
}

impl Database {
    /// Opens a SQLite file on the calling thread.
    ///
    /// The file is created when `path` is missing. A symlink is refused.
    /// This function does not read a migration journal and does not run
    /// migration SQL.
    ///
    /// # Examples
    ///
    /// ```
    /// let path = std::env::temp_dir().join(format!(
    ///     "dbm-sqlite-doc-{}",
    ///     std::process::id()
    /// ));
    /// let _ = std::fs::remove_file(&path);
    /// let database = dbm_sqlite::Database::open(&path)?;
    /// let value: i64 = database
    ///     .connection()
    ///     .query_row("SELECT 1", [], |row| row.get(0))
    ///     .expect("select one");
    /// assert_eq!(value, 1);
    /// drop(database);
    /// std::fs::remove_file(&path).expect("remove doc database");
    /// # Ok::<(), dbm_sqlite::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error when the path cannot be opened. A missing parent
    /// directory, a symlink, and a directory path fail without creating a
    /// database file.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        // Refuse a symlink so the opened path is the file the caller named.
        let flags = OpenFlags::default() | OpenFlags::SQLITE_OPEN_NOFOLLOW;
        let connection = match Connection::open_with_flags(path, flags) {
            Ok(connection) => connection,
            Err(source) => {
                return Err(Error::open(path, source));
            }
        };
        Ok(Self { connection })
    }

    /// Borrows the open connection.
    ///
    /// The caller runs SQL. This crate does not provide a query builder.
    #[must_use]
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::Database;

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

    #[test]
    fn open_creates_a_file_and_reads_it_back() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");

        let database = Database::open(&path).expect("open new file");
        database
            .connection()
            .execute("CREATE TABLE note (id INTEGER PRIMARY KEY)", [])
            .expect("create table");
        let created: i64 = database
            .connection()
            .query_row("SELECT COUNT(*) FROM note", [], |row| row.get(0))
            .expect("count rows");
        assert_eq!(created, 0);
        drop(database);

        let reopened = Database::open(&path).expect("reopen file");
        let value: i64 = reopened
            .connection()
            .query_row("SELECT 1", [], |row| row.get(0))
            .expect("select one");
        assert_eq!(value, 1);
        let metadata = fs::metadata(&path).expect("stat database");
        assert!(metadata.is_file());
        assert!(metadata.len() > 0);
    }

    #[test]
    fn open_refuses_a_missing_parent_directory() {
        let scratch = Scratch::new();
        let path = scratch.path.join("missing").join("app.sqlite");

        let opened = Database::open(&path);
        assert!(opened.is_err());
        assert!(!scratch.path.join("missing").exists());
    }

    #[test]
    fn open_refuses_a_directory() {
        let scratch = Scratch::new();
        let path = scratch.path.join("not-a-file");
        fs::create_dir(&path).expect("create directory");

        let opened = Database::open(&path).expect_err("directory opens");
        assert_eq!(opened.path(), path.as_path());
    }

    #[cfg(unix)]
    #[test]
    fn open_refuses_a_symlink() {
        let scratch = Scratch::new();
        let target = scratch.path.join("target.sqlite");
        let link = scratch.path.join("link.sqlite");
        fs::write(&target, b"").expect("write empty target");
        std::os::unix::fs::symlink(&target, &link).expect("create symlink");

        let opened = Database::open(&link).expect_err("symlink opens");
        assert_eq!(opened.path(), link.as_path());
        let bytes = fs::read(&target).expect("read target");
        assert_eq!(bytes, b"");

        let database = Database::open(&target).expect("open target path");
        let value: i64 = database
            .connection()
            .query_row("SELECT 1", [], |row| row.get(0))
            .expect("select one");
        assert_eq!(value, 1);
    }
}
