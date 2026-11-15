use std::error::Error as StdError;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

/// Failure while opening a SQLite file or applying its journal.
#[derive(Debug)]
pub struct Error {
    path: PathBuf,
    kind: ErrorKind,
}

/// Why [`Error`](crate::Error) stopped the open.
#[derive(Debug)]
#[non_exhaustive]
pub enum ErrorKind {
    /// SQLite refused to open the file or to run journal bookkeeping.
    Open(rusqlite::Error),
    /// Filesystem metadata or the parent directory blocked the open.
    Io(io::Error),
    /// `path` is a symlink. The target was not opened.
    Symlink,
    /// `path` is a directory.
    Directory,
    /// The file has tables and no `dbm_sqlite_journal`.
    UnrecognizedFile,
    /// `version` is not a 20-digit UTC timestamp.
    InvalidVersion { version: String },
    /// The same version appears twice in the migrations passed to open.
    DuplicateVersion { version: String },
    /// The migration has no SQL statement.
    EmptySql { version: String },
    /// The migration SQL is not valid to run inside the journal transaction.
    InvalidSql { version: String, reason: String },
    /// The migration SQL would leave the journal transaction.
    UnsafeSql { version: String, keyword: String },
    /// An applied version is absent from the migrations passed to open.
    MissingMigration { version: String },
    /// An applied version was presented again with different SQL bytes.
    ChecksumMismatch {
        version: String,
        applied_checksum: String,
        provided_checksum: String,
    },
    /// A journal row is `started` or has a state this version does not know.
    UnknownExecution { version: String, state: String },
    /// A `failed` journal row is still present and the version was not offered again.
    UnresolvedFailure { version: String },
    /// The journal changed between the read and the write, so no migration SQL ran.
    JournalConflict { version: String },
    /// Migration SQL failed. The statement transaction was rolled back.
    Migration {
        version: String,
        source: rusqlite::Error,
    },
    /// This process or another already owns the file. This open did not write.
    Owned,
    /// [`Database::call`](crate::Database::call) found no live owner.
    NoOwner,
    /// The owner stopped while SQL was running. This owner accepts no further SQL.
    OwnerInterrupted,
}

/// The applied checksum and the checksum of the SQL passed to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChecksumMismatch<'a> {
    pub version: &'a str,
    pub applied_checksum: &'a str,
    pub provided_checksum: &'a str,
}

/// A journal execution that cannot be retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownExecution<'a> {
    pub version: &'a str,
    pub state: &'a str,
}

impl Error {
    pub(crate) fn new(path: &Path, kind: ErrorKind) -> Self {
        Self {
            path: path.to_path_buf(),
            kind,
        }
    }

    /// Path the caller asked to open.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Why the open stopped.
    #[must_use]
    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    /// Applied and provided checksums when the open stopped for that reason.
    #[must_use]
    pub fn checksum_mismatch(&self) -> Option<ChecksumMismatch<'_>> {
        match &self.kind {
            ErrorKind::ChecksumMismatch {
                version,
                applied_checksum,
                provided_checksum,
            } => Some(ChecksumMismatch {
                version,
                applied_checksum,
                provided_checksum,
            }),
            _ => None,
        }
    }

    /// Version whose execution is still unknown.
    #[must_use]
    pub fn unknown_execution(&self) -> Option<UnknownExecution<'_>> {
        match &self.kind {
            ErrorKind::UnknownExecution { version, state } => {
                Some(UnknownExecution { version, state })
            }
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self.path.display();
        match &self.kind {
            ErrorKind::Open(source) => {
                write!(
                    formatter,
                    "failed to open SQLite database at {path}: {source}"
                )
            }
            ErrorKind::Io(source) => {
                write!(
                    formatter,
                    "failed to open SQLite database at {path}: {source}"
                )
            }
            ErrorKind::Symlink => {
                write!(formatter, "refusing to follow a symlink at {path}")
            }
            ErrorKind::Directory => {
                write!(formatter, "refusing to open a directory at {path}")
            }
            ErrorKind::UnrecognizedFile => {
                write!(
                    formatter,
                    "database at {path} has tables and no dbm_sqlite_journal; the file was left unchanged"
                )
            }
            ErrorKind::InvalidVersion { version } => {
                write!(
                    formatter,
                    "migration {version} is not a 20-digit UTC timestamp; database at {path} was left unchanged"
                )
            }
            ErrorKind::DuplicateVersion { version } => {
                write!(
                    formatter,
                    "migration {version} is repeated; database at {path} was left unchanged"
                )
            }
            ErrorKind::EmptySql { version } => {
                write!(
                    formatter,
                    "migration {version} has no SQL; database at {path} was left unchanged"
                )
            }
            ErrorKind::InvalidSql { version, reason } => {
                write!(
                    formatter,
                    "migration {version} has invalid SQL ({reason}); database at {path} was left unchanged"
                )
            }
            ErrorKind::UnsafeSql { version, keyword } => {
                write!(
                    formatter,
                    "migration {version} contains {keyword}, which would leave the journal transaction; database at {path} was left unchanged"
                )
            }
            ErrorKind::MissingMigration { version } => {
                write!(
                    formatter,
                    "applied migration {version} is missing from this open; database at {path} was left unchanged"
                )
            }
            ErrorKind::ChecksumMismatch {
                version,
                applied_checksum,
                provided_checksum,
            } => {
                write!(
                    formatter,
                    "checksum mismatch for applied migration {version} at {path}: applied {applied_checksum}, provided {provided_checksum}"
                )
            }
            ErrorKind::UnknownExecution { version, state } => {
                write!(
                    formatter,
                    "unknown execution for migration {version} at {path}: journal state is {state}; writes are blocked"
                )
            }
            ErrorKind::UnresolvedFailure { version } => {
                write!(
                    formatter,
                    "migration {version} failed and is still in the journal at {path}; writes are blocked"
                )
            }
            ErrorKind::JournalConflict { version } => {
                write!(
                    formatter,
                    "journal for migration {version} changed before the write at {path}; the file was left unchanged"
                )
            }
            ErrorKind::Migration { version, source } => {
                write!(
                    formatter,
                    "migration {version} failed at {path}: {source}; the migration transaction was rolled back"
                )
            }
            ErrorKind::Owned => {
                write!(
                    formatter,
                    "database at {path} already has a connection owner; this open did not write"
                )
            }
            ErrorKind::NoOwner => {
                write!(
                    formatter,
                    "database at {path} has no connection owner; the file was left unchanged"
                )
            }
            ErrorKind::OwnerInterrupted => {
                write!(
                    formatter,
                    "connection owner for {path} stopped during SQL; further use of this owner is blocked"
                )
            }
        }
    }
}

impl From<rusqlite::Error> for ErrorKind {
    fn from(source: rusqlite::Error) -> Self {
        Self::Open(source)
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match &self.kind {
            ErrorKind::Open(source) | ErrorKind::Migration { source, .. } => Some(source),
            ErrorKind::Io(source) => Some(source),
            _ => None,
        }
    }
}
