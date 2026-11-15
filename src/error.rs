use std::error::Error as StdError;
use std::fmt;
use std::path::{Path, PathBuf};

/// Failure while opening a SQLite file.
#[derive(Debug)]
pub struct Error {
    path: PathBuf,
    source: rusqlite::Error,
}

impl Error {
    pub(crate) fn open(path: &Path, source: rusqlite::Error) -> Self {
        Self {
            path: path.to_path_buf(),
            source,
        }
    }

    /// Path the caller asked to open.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "failed to open SQLite database at {}: {}",
            self.path.display(),
            self.source
        )
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.source)
    }
}
