use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};

use crate::Migration;
use crate::error::{Error, ErrorKind};
use crate::journal::{self, open_database};

#[cfg(any(target_os = "linux", target_os = "android"))]
const NOFOLLOW: i32 = 0x20000;

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "tvos",
    target_os = "watchos"
))]
const NOFOLLOW: i32 = 0x0100;

enum Slot {
    Opening,
    Ready(Weak<Owner>),
}

struct Live {
    connection: Option<rusqlite::Connection>,
    lock_file: Option<File>,
}

struct Owner {
    key: PathBuf,
    live: Mutex<Live>,
}

impl Drop for Owner {
    fn drop(&mut self) {
        let mut owners = owners();
        match owners.get(&self.key) {
            Some(Slot::Ready(owner)) if owner.upgrade().is_some() => {}
            _ => {
                owners.remove(&self.key);
            }
        }
        let live = match self.live.get_mut() {
            Ok(live) => live,
            Err(poisoned) => poisoned.into_inner(),
        };
        live.connection.take();
        live.lock_file.take();
    }
}

struct Opening {
    key: PathBuf,
    armed: bool,
}

impl Drop for Opening {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let mut owners = owners();
        if matches!(owners.get(&self.key), Some(Slot::Opening)) {
            owners.remove(&self.key);
        }
    }
}

/// Borrow of the single connection.
///
/// The guard is the only user of that connection until it is dropped.
/// Another thread that asks the owner waits. Do not call [`Database::call`]
/// or [`Database::connection`] again while this guard is alive on the same
/// thread.
#[must_use = "dropping the guard releases the connection"]
pub struct ConnectionGuard<'a> {
    live: MutexGuard<'a, Live>,
}

impl Deref for ConnectionGuard<'_> {
    type Target = rusqlite::Connection;

    fn deref(&self) -> &Self::Target {
        self.live
            .connection
            .as_ref()
            .expect("owner connection is open while a guard borrows it")
    }
}

impl DerefMut for ConnectionGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.live
            .connection
            .as_mut()
            .expect("owner connection is open while a guard borrows it")
    }
}

impl std::fmt::Debug for ConnectionGuard<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ConnectionGuard")
    }
}

/// One SQLite connection.
///
/// `open` migrates, then this value owns the connection. A clone asks that
/// same owner and does not open another connection. Dropping the last clone
/// closes the file.
#[derive(Clone)]
#[must_use = "dropping the last owner closes the file"]
pub struct Database {
    path: PathBuf,
    owner: Arc<Owner>,
}

impl std::fmt::Debug for Database {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Database").finish_non_exhaustive()
    }
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
    /// A second `open` of the same file, in this process or another, returns
    /// [`ErrorKind::Owned`](crate::ErrorKind::Owned) and does not write. The
    /// caller uses [`Database::call`] or a clone of the owner instead.
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
    ///     .connection()?
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
    /// Returns an error when the path cannot be opened, the file already has
    /// an owner, the journal does not match `migrations`, or a migration
    /// fails. The [`Error`](crate::Error) display includes a checksum mismatch
    /// and an unknown execution. A missing parent directory, a symlink, and a
    /// directory path fail without creating a database file.
    pub fn open(path: impl AsRef<Path>, migrations: &[Migration<'_>]) -> Result<Self, Error> {
        let path = path.as_ref();
        journal::prepare(migrations).map_err(|kind| Error::new(path, kind))?;
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(Error::new(path, ErrorKind::Symlink));
            }
            Ok(metadata) if metadata.is_dir() => {
                return Err(Error::new(path, ErrorKind::Directory));
            }
            Ok(_) => {}
            Err(source) if source.kind() == io::ErrorKind::NotFound => {
                journal::ensure_parent_exists(path).map_err(|kind| Error::new(path, kind))?;
            }
            Err(source) => return Err(Error::new(path, ErrorKind::Io(source))),
        }

        let key = owner_key(path).map_err(|kind| Error::new(path, kind))?;
        {
            let mut owners = owners();
            match owners.get(&key) {
                Some(Slot::Opening) => return Err(Error::new(path, ErrorKind::Owned)),
                Some(Slot::Ready(owner)) if owner.upgrade().is_some() => {
                    return Err(Error::new(path, ErrorKind::Owned));
                }
                Some(_) => {
                    owners.remove(&key);
                }
                None => {}
            }
            owners.insert(key.clone(), Slot::Opening);
        }
        let mut opening = Opening {
            key: key.clone(),
            armed: true,
        };

        let lock_path = lock_path(&key);
        let lock_file = acquire_lock(path, &lock_path)?;
        let connection = open_database(path, migrations)?;
        let owner = Arc::new(Owner {
            key: key.clone(),
            live: Mutex::new(Live {
                connection: Some(connection),
                lock_file: Some(lock_file),
            }),
        });
        {
            let mut owners = owners();
            owners.insert(key, Slot::Ready(Arc::downgrade(&owner)));
        }
        opening.armed = false;
        Ok(Self {
            path: path.to_path_buf(),
            owner,
        })
    }

    /// Borrows the open connection.
    ///
    /// The caller runs SQL on the calling thread. This crate does not provide
    /// a query builder. The guard keeps the connection until it is dropped.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorKind::OwnerInterrupted`](crate::ErrorKind::OwnerInterrupted)
    /// when a previous user of this owner stopped while holding the connection.
    pub fn connection(&self) -> Result<ConnectionGuard<'_>, Error> {
        let live = self
            .owner
            .live
            .lock()
            .map_err(|_| Error::new(&self.path, ErrorKind::OwnerInterrupted))?;
        if live.connection.is_none() {
            return Err(Error::new(&self.path, ErrorKind::OwnerInterrupted));
        }
        Ok(ConnectionGuard { live })
    }

    /// Runs `operation` on the connection owned for `path`.
    ///
    /// A worker or isolate asks here. This does not open a second connection
    /// and does not apply migrations. When no owner is alive, the file is left
    /// unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorKind::NoOwner`](crate::ErrorKind::NoOwner) when `open`
    /// has not created an owner for `path`. Returns
    /// [`ErrorKind::OwnerInterrupted`](crate::ErrorKind::OwnerInterrupted)
    /// when that owner stopped during SQL. A symlink is refused.
    pub fn call<T>(
        path: impl AsRef<Path>,
        operation: impl FnOnce(&mut rusqlite::Connection) -> T,
    ) -> Result<T, Error> {
        let path = path.as_ref();
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(Error::new(path, ErrorKind::Symlink));
            }
            Ok(metadata) if metadata.is_dir() => {
                return Err(Error::new(path, ErrorKind::Directory));
            }
            Ok(_) => {}
            Err(source) if source.kind() == io::ErrorKind::NotFound => {
                return Err(Error::new(path, ErrorKind::NoOwner));
            }
            Err(source) => return Err(Error::new(path, ErrorKind::Io(source))),
        }
        let key = owner_key(path).map_err(|kind| Error::new(path, kind))?;
        let owner = {
            let owners = owners();
            match owners.get(&key) {
                Some(Slot::Ready(owner)) => owner.upgrade(),
                _ => None,
            }
        };
        let Some(owner) = owner else {
            return Err(Error::new(path, ErrorKind::NoOwner));
        };
        let mut live = owner
            .live
            .lock()
            .map_err(|_| Error::new(path, ErrorKind::OwnerInterrupted))?;
        let Some(connection) = live.connection.as_mut() else {
            return Err(Error::new(path, ErrorKind::OwnerInterrupted));
        };
        Ok(operation(connection))
    }
}

fn owners() -> MutexGuard<'static, HashMap<PathBuf, Slot>> {
    static OWNERS: OnceLock<Mutex<HashMap<PathBuf, Slot>>> = OnceLock::new();
    OWNERS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn owner_key(path: &Path) -> Result<PathBuf, ErrorKind> {
    if path.exists() {
        return fs::canonicalize(path).map_err(ErrorKind::Io);
    }
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let Some(name) = path.file_name() else {
        return Err(ErrorKind::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "database path has no file name",
        )));
    };
    Ok(fs::canonicalize(parent).map_err(ErrorKind::Io)?.join(name))
}

fn lock_path(key: &Path) -> PathBuf {
    let mut name = key.as_os_str().to_os_string();
    name.push(".dbm-owner");
    PathBuf::from(name)
}

fn acquire_lock(database: &Path, path: &Path) -> Result<File, Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(Error::new(database, ErrorKind::Symlink));
        }
        Ok(metadata) if metadata.is_dir() => {
            return Err(Error::new(database, ErrorKind::Directory));
        }
        Ok(_) => {}
        Err(source) if source.kind() == io::ErrorKind::NotFound => {}
        Err(source) => return Err(Error::new(database, ErrorKind::Io(source))),
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos"
    ))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(NOFOLLOW);
    }
    let file = options
        .open(path)
        .map_err(|source| Error::new(database, ErrorKind::Io(source)))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(std::fs::TryLockError::WouldBlock) => Err(Error::new(database, ErrorKind::Owned)),
        Err(std::fs::TryLockError::Error(source)) => {
            Err(Error::new(database, ErrorKind::Io(source)))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{Database, lock_path, owner_key};
    use crate::Migration;
    use crate::error::ErrorKind;

    const VERSION_ONE: &str = "20261115000000000001";
    const VERSION_TWO: &str = "20261115000000000002";

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
                "dbm-sqlite-owner-{}-{}-{}",
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

    #[test]
    fn a_second_open_does_not_write_and_a_worker_asks_the_owner() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        let migrations = [migration(
            VERSION_ONE,
            "CREATE TABLE note (id INTEGER PRIMARY KEY);",
        )];
        let database = Database::open(&path, &migrations).expect("owner");
        let before = fs::read(&path).expect("read owned file");
        let path_for_worker = path.clone();

        let second = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    Database::open(
                        &path_for_worker,
                        &[
                            migrations[0],
                            migration(
                                VERSION_TWO,
                                "CREATE TABLE second_sentinel_table (id INTEGER PRIMARY KEY);",
                            ),
                        ],
                    )
                })
                .join()
                .expect("second open thread")
        });
        assert!(matches!(
            second.expect_err("owned").kind(),
            ErrorKind::Owned
        ));
        assert_eq!(fs::read(&path).expect("read preserved file"), before);

        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    Database::call(&path_for_worker, |connection| {
                        connection
                            .execute("INSERT INTO note (id) VALUES (1)", [])
                            .expect("insert through the owner");
                    })
                    .expect("worker call")
                })
                .join()
                .expect("worker thread");
        });
        let count: i64 = database
            .connection()
            .expect("owner")
            .query_row("SELECT COUNT(*) FROM note", [], |row| row.get(0))
            .expect("count notes");
        assert_eq!(count, 1);
        let report = crate::doctor(&path, "1.2.3", &migrations);
        assert_eq!(report.file(), crate::FileStatus::Present);
        assert!(!report.to_string().contains("CREATE TABLE"));

        let clone = database.clone();
        drop(database);
        let still_owned = Database::open(&path, &migrations).expect_err("clone still owns");
        assert!(matches!(still_owned.kind(), ErrorKind::Owned));
        drop(clone);

        let reopened = Database::open(&path, &migrations).expect("reopen after the last owner");
        drop(reopened);
        let names = crate::journal::table_names(
            &crate::journal::open_readonly(&path).expect("read tables"),
        )
        .expect("table names");
        assert!(!names.iter().any(|name| name == "second_sentinel_table"));
    }

    #[test]
    fn call_without_an_owner_does_not_create_the_file() {
        let scratch = Scratch::new();
        let path = scratch.path.join("PATH_SENTINEL.sqlite");
        let error = Database::call(&path, |_connection| ()).expect_err("no owner");
        assert!(matches!(error.kind(), ErrorKind::NoOwner));
        assert!(!path.exists());
        assert!(!lock_path(&owner_key(&path).expect("key")).exists());
    }

    #[test]
    fn an_interrupted_owner_blocks_later_sql() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        let database = Database::open(
            &path,
            &[migration(
                VERSION_ONE,
                "CREATE TABLE note (id INTEGER PRIMARY KEY);",
            )],
        )
        .expect("owner");
        let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = database.connection().expect("owner");
            panic!("stop the owner");
        }));
        assert!(interrupted.is_err());
        let error = database.connection().expect_err("interrupted owner");
        assert!(matches!(error.kind(), ErrorKind::OwnerInterrupted));
        let call = Database::call(&path, |connection| {
            connection.execute("INSERT INTO note (id) VALUES (1)", [])
        });
        assert!(matches!(
            call.expect_err("call is blocked").kind(),
            ErrorKind::OwnerInterrupted
        ));
        let count: i64 = crate::journal::open_readonly(&path)
            .expect("read note")
            .query_row("SELECT COUNT(*) FROM note", [], |row| row.get(0))
            .expect("count notes");
        assert_eq!(count, 0);
    }

    #[test]
    fn another_process_holding_the_owner_lock_does_not_create_the_file() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        let key = owner_key(&path).expect("key");
        let lock = lock_path(&key);
        let source = scratch.path.join("hold.rs");
        fs::write(
            &source,
            "fn main() {\n    let path = std::env::args().nth(1).expect(\"path\");\n    let file = std::fs::OpenOptions::new()\n        .read(true)\n        .write(true)\n        .create(true)\n        .open(path)\n        .expect(\"open\");\n    file.try_lock().expect(\"lock\");\n    println!(\"held\");\n    let _ = std::io::Write::flush(&mut std::io::stdout());\n    std::thread::sleep(std::time::Duration::from_secs(60));\n}\n",
        )
        .expect("write helper");
        let binary = scratch.path.join("hold");
        let compiled = std::process::Command::new("rustc")
            .arg("--edition")
            .arg("2024")
            .arg(&source)
            .arg("-o")
            .arg(&binary)
            .output()
            .expect("run rustc");
        assert!(
            compiled.status.success(),
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        let child = std::process::Command::new(&binary)
            .arg(&lock)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("spawn holder");
        struct Stop(Option<std::process::Child>);
        impl Drop for Stop {
            fn drop(&mut self) {
                if let Some(child) = self.0.as_mut() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
        let mut stop = Stop(Some(child));
        let held = stop.0.as_mut().expect("child");
        let mut line = String::new();
        std::io::BufRead::read_line(
            &mut std::io::BufReader::new(held.stdout.as_mut().expect("stdout")),
            &mut line,
        )
        .expect("read held");
        assert_eq!(line.trim(), "held");

        let error = Database::open(
            &path,
            &[migration(
                VERSION_ONE,
                "CREATE TABLE note (id INTEGER PRIMARY KEY);",
            )],
        )
        .expect_err("other process owns the file");
        assert!(matches!(error.kind(), ErrorKind::Owned));
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn call_does_not_follow_a_symlink() {
        let scratch = Scratch::new();
        let target = scratch.path.join("TARGET_SENTINEL");
        let link = scratch.path.join("link.sqlite");
        fs::write(&target, b"TARGET_SENTINEL_BYTES").expect("write target");
        std::os::unix::fs::symlink(&target, &link).expect("create symlink");
        let error = Database::call(&link, |_connection| ()).expect_err("symlink");
        assert!(matches!(error.kind(), ErrorKind::Symlink));
        assert_eq!(
            fs::read(&target).expect("read target"),
            b"TARGET_SENTINEL_BYTES"
        );
    }
}
