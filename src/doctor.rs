use std::fs;
use std::io;
use std::path::Path;

use crate::Migration;
use crate::error::ErrorKind;
use crate::journal::{self, Catalog, JournalRow};

/// How [`doctor`](crate::doctor) classified the path without opening a symlink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FileStatus {
    /// No file exists at the path.
    Missing,
    /// The path is a symlink. The target was not opened.
    Symlink,
    /// The path is a directory.
    Directory,
    /// A regular file is present and was read, or it is not a SQLite database.
    Present,
    /// The file could not be read. Its bytes were not changed.
    Unreadable,
}

/// One journal fact safe to show in a crash report.
///
/// Version, checksum, and state strings are included only when they match the
/// journal's own shape. Anything else is omitted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Finding {
    /// `version` is not a 20-digit UTC timestamp.
    InvalidVersion { version: Option<String> },
    /// The same version appears twice in the migrations passed to doctor.
    DuplicateVersion { version: Option<String> },
    /// The migration has no SQL statement.
    EmptySql { version: Option<String> },
    /// The migration SQL is not valid to run inside the journal transaction.
    InvalidSql { version: Option<String> },
    /// The migration SQL would leave the journal transaction.
    UnsafeSql {
        version: Option<String>,
        keyword: Option<String>,
    },
    /// The file has tables and no `dbm_sqlite_journal`.
    UnrecognizedFile,
    /// The bytes are not a SQLite database.
    NotADatabase,
    /// A keyed file could not be read: the key may be wrong or the file invalid.
    EncryptedUnreadable,
    /// `dbm_sqlite_journal` exists and its rows could not be read.
    JournalUnreadable,
    /// An applied version was presented again with different SQL bytes.
    ChecksumMismatch {
        version: Option<String>,
        applied_checksum: Option<String>,
        provided_checksum: Option<String>,
    },
    /// A journal row is `started` or has a state this version does not treat as settled.
    UnknownExecution {
        version: Option<String>,
        state: Option<String>,
    },
    /// A `failed` journal row is still present and the version was offered again.
    Failed { version: Option<String> },
    /// A `failed` journal row is still present and the version was not offered again.
    UnresolvedFailure { version: Option<String> },
    /// An applied version is absent from the migrations passed to doctor.
    MissingMigration { version: Option<String> },
    /// The version is in the catalog and has no journal row.
    Pending { version: Option<String> },
}

/// The newest journal row in version order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalHead {
    version: Option<String>,
    state: Option<String>,
    checksum: Option<String>,
}

impl JournalHead {
    /// Journal version when it is 20 digits.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    /// Journal state when it is a short lowercase word.
    #[must_use]
    pub fn state(&self) -> Option<&str> {
        self.state.as_deref()
    }

    /// Stored checksum when it is `sha256:raw:v1` and 64 lowercase hex digits.
    #[must_use]
    pub fn checksum(&self) -> Option<&str> {
        self.checksum.as_deref()
    }
}

/// Read-only diagnosis of a SQLite file.
///
/// [`Display`](std::fmt::Display) is the crash-report text. It contains the
/// crate version, the bundled SQLite version, the caller-supplied application
/// version, the journal head, and the findings. It does not contain the path,
/// migration SQL, table names, or rows from application tables.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "the doctor report is the crash-report text"]
pub struct DoctorReport {
    library_version: &'static str,
    sqlite_version: &'static str,
    app_version: Option<String>,
    file: FileStatus,
    other_tables: u32,
    journal_rows: u32,
    head: Option<JournalHead>,
    findings: Vec<Finding>,
}

impl DoctorReport {
    /// Crate version that produced the report.
    #[must_use]
    pub fn library_version(&self) -> &str {
        self.library_version
    }

    /// Bundled SQLite library version.
    #[must_use]
    pub fn sqlite_version(&self) -> &str {
        self.sqlite_version
    }

    /// Application version supplied by the caller, when it was safe to copy.
    #[must_use]
    pub fn app_version(&self) -> Option<&str> {
        self.app_version.as_deref()
    }

    /// Classification of the path.
    #[must_use]
    pub fn file(&self) -> FileStatus {
        self.file
    }

    /// Number of tables other than `dbm_sqlite_journal`.
    ///
    /// The names are not part of the report.
    #[must_use]
    pub fn other_tables(&self) -> u32 {
        self.other_tables
    }

    /// Number of journal rows that were read.
    #[must_use]
    pub fn journal_rows(&self) -> u32 {
        self.journal_rows
    }

    /// Newest journal row in version order.
    #[must_use]
    pub fn head(&self) -> Option<&JournalHead> {
        self.head.as_ref()
    }

    /// Journal and catalog findings, in version order.
    #[must_use]
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    fn blank(app_version: &str) -> Self {
        Self {
            library_version: env!("CARGO_PKG_VERSION"),
            sqlite_version: rusqlite::version(),
            app_version: app_label(app_version),
            file: FileStatus::Missing,
            other_tables: 0,
            journal_rows: 0,
            head: None,
            findings: Vec::new(),
        }
    }
}

impl std::fmt::Display for DoctorReport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(formatter, "dbm-sqlite {}", self.library_version)?;
        writeln!(formatter, "sqlite {}", self.sqlite_version)?;
        match &self.app_version {
            Some(version) => writeln!(formatter, "app {version}")?,
            None => writeln!(formatter, "app omitted")?,
        }
        writeln!(formatter, "file {}", file_label(self.file))?;
        writeln!(formatter, "other_tables {}", self.other_tables)?;
        writeln!(formatter, "journal_rows {}", self.journal_rows)?;
        match &self.head {
            Some(head) => writeln!(
                formatter,
                "head {} {} {}",
                shown(&head.version),
                shown(&head.state),
                shown(&head.checksum)
            )?,
            None => writeln!(formatter, "head none")?,
        }
        if self.findings.is_empty() {
            writeln!(formatter, "finding none")?;
            return Ok(());
        }
        for finding in &self.findings {
            writeln!(formatter, "finding {finding}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for Finding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidVersion { version } => {
                write!(formatter, "invalid version {}", shown(version))
            }
            Self::DuplicateVersion { version } => {
                write!(formatter, "duplicate version {}", shown(version))
            }
            Self::EmptySql { version } => write!(formatter, "empty sql {}", shown(version)),
            Self::InvalidSql { version } => write!(formatter, "invalid sql {}", shown(version)),
            Self::UnsafeSql { version, keyword } => {
                write!(
                    formatter,
                    "unsafe sql {} {}",
                    shown(version),
                    shown(keyword)
                )
            }
            Self::UnrecognizedFile => write!(formatter, "unrecognized file"),
            Self::NotADatabase => write!(formatter, "not a database"),
            Self::EncryptedUnreadable => write!(formatter, "encrypted file unreadable"),
            Self::JournalUnreadable => write!(formatter, "journal unreadable"),
            Self::ChecksumMismatch {
                version,
                applied_checksum,
                provided_checksum,
            } => write!(
                formatter,
                "checksum mismatch {} applied {} provided {}",
                shown(version),
                shown(applied_checksum),
                shown(provided_checksum)
            ),
            Self::UnknownExecution { version, state } => {
                write!(
                    formatter,
                    "unknown execution {} state {}",
                    shown(version),
                    shown(state)
                )
            }
            Self::Failed { version } => write!(formatter, "failed {}", shown(version)),
            Self::UnresolvedFailure { version } => {
                write!(formatter, "unresolved failure {}", shown(version))
            }
            Self::MissingMigration { version } => {
                write!(formatter, "missing migration {}", shown(version))
            }
            Self::Pending { version } => write!(formatter, "pending {}", shown(version)),
        }
    }
}

/// Reads the SQLite file and returns a short diagnosis.
///
/// The caller passes `app_version`. A single line of at most 128 bytes, with
/// no control characters, is copied into the report. Any other value is
/// omitted. The path is not copied. Migration SQL, table names, and rows from
/// application tables are not copied.
///
/// This function does not create a missing file, does not create a missing
/// parent directory, does not follow a symlink, and does not apply or retry a
/// migration.
///
/// # Examples
///
/// ```
/// let path = std::env::temp_dir().join(format!(
///     "dbm-sqlite-doctor-doc-{}",
///     std::process::id()
/// ));
/// let _ = std::fs::remove_file(&path);
/// let migrations = [dbm_sqlite::Migration {
///     version: "20261115000000000000",
///     sql: "CREATE TABLE note (id INTEGER PRIMARY KEY);",
/// }];
/// let database = dbm_sqlite::Database::open(&path, &migrations)?;
/// drop(database);
/// let report = dbm_sqlite::doctor(&path, "1.0.0", &migrations);
/// let text = report.to_string();
/// assert!(text.contains("dbm-sqlite 0.1.0"));
/// assert!(text.contains("app 1.0.0"));
/// assert!(text.contains("head 20261115000000000000 applied "));
/// assert!(text.contains("finding none"));
/// assert!(!text.contains("CREATE TABLE"));
/// std::fs::remove_file(&path).expect("remove doc database");
/// # Ok::<(), dbm_sqlite::Error>(())
/// ```
pub fn doctor(
    path: impl AsRef<Path>,
    app_version: &str,
    migrations: &[Migration<'_>],
) -> DoctorReport {
    doctor_inner(path.as_ref(), app_version, migrations, None)
}

/// Diagnoses an encrypted database without applying migrations.
///
/// A wrong key and an invalid encrypted file are deliberately reported alike.
/// Do not put secrets in `app_version`. The key is not stored in the report.
///
/// # Errors
///
/// Rejects an empty key or a key containing NUL without opening the path.
#[cfg(feature = "sqlcipher")]
pub fn doctor_encrypted(
    path: impl AsRef<Path>,
    app_version: &str,
    migrations: &[Migration<'_>],
    key: &str,
) -> Result<DoctorReport, crate::Error> {
    if key.is_empty() || key.contains('\0') {
        return Err(crate::Error::new(
            path.as_ref(),
            ErrorKind::InvalidEncryptionKey,
        ));
    }
    Ok(doctor_inner(
        path.as_ref(),
        app_version,
        migrations,
        Some(key),
    ))
}

fn doctor_inner(
    path: &Path,
    app_version: &str,
    migrations: &[Migration<'_>],
    key: Option<&str>,
) -> DoctorReport {
    let mut report = DoctorReport::blank(app_version);
    let catalog = match journal::prepare(migrations) {
        Ok(catalog) => Some(catalog),
        Err(kind) => {
            push_catalog_error(&mut report, kind);
            None
        }
    };
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            report.file = FileStatus::Symlink;
        }
        Ok(metadata) if metadata.is_dir() => {
            report.file = FileStatus::Directory;
        }
        Ok(_) => inspect_file(&mut report, path, catalog.as_ref(), key),
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            report.file = FileStatus::Missing;
            if let Some(catalog) = catalog.as_ref() {
                push_pending(&mut report.findings, catalog);
            }
        }
        Err(_) => {
            report.file = FileStatus::Unreadable;
        }
    }
    report
}

fn inspect_file(
    report: &mut DoctorReport,
    path: &Path,
    catalog: Option<&Catalog>,
    key: Option<&str>,
) {
    let connection = match journal::open_readonly_with_key(path, key) {
        Ok(connection) => connection,
        Err(kind) => {
            if open_is_not_a_database(&kind) {
                report.file = FileStatus::Present;
                if key.is_some() {
                    report.findings.push(Finding::EncryptedUnreadable);
                } else {
                    report.findings.push(Finding::NotADatabase);
                }
            } else {
                report.file = FileStatus::Unreadable;
            }
            return;
        }
    };
    report.file = FileStatus::Present;
    let names = match journal::table_names(&connection) {
        Ok(names) => names,
        Err(source) => {
            if source.sqlite_error_code() == Some(rusqlite::ErrorCode::NotADatabase) {
                if key.is_some() {
                    report.findings.push(Finding::EncryptedUnreadable);
                } else {
                    report.findings.push(Finding::NotADatabase);
                }
            } else {
                report.file = FileStatus::Unreadable;
            }
            return;
        }
    };
    let mut has_journal = false;
    for name in &names {
        if name == journal::JOURNAL_TABLE {
            has_journal = true;
        } else {
            report.other_tables = report.other_tables.saturating_add(1);
        }
    }
    if !has_journal {
        if report.other_tables > 0 {
            report.findings.push(Finding::UnrecognizedFile);
            return;
        }
        if let Some(catalog) = catalog {
            push_pending(&mut report.findings, catalog);
        }
        return;
    }
    let rows = match journal::read_journal(&connection) {
        Ok(rows) => rows,
        Err(_) => {
            report.findings.push(Finding::JournalUnreadable);
            return;
        }
    };
    report.journal_rows = u32::try_from(rows.len()).unwrap_or(u32::MAX);
    if let Some(head) = rows.last() {
        report.head = Some(journal_head(head));
    }
    compare_journal(report, &rows, catalog);
}

fn compare_journal(report: &mut DoctorReport, rows: &[JournalRow], catalog: Option<&Catalog>) {
    let Some(catalog) = catalog else {
        for row in rows {
            push_state_without_catalog(&mut report.findings, row);
        }
        return;
    };
    for row in rows {
        let migration = catalog
            .migrations
            .iter()
            .find(|migration| migration.version == row.version);
        match row.state.as_str() {
            "applied" => match migration {
                None => report.findings.push(Finding::MissingMigration {
                    version: label_version(&row.version),
                }),
                Some(migration) if migration.checksum != row.checksum => {
                    report.findings.push(Finding::ChecksumMismatch {
                        version: label_version(&row.version),
                        applied_checksum: label_checksum(&row.checksum),
                        provided_checksum: label_checksum(&migration.checksum),
                    });
                }
                Some(_) => {}
            },
            "failed" => {
                if migration.is_none() {
                    report.findings.push(Finding::UnresolvedFailure {
                        version: label_version(&row.version),
                    });
                } else {
                    report.findings.push(Finding::Failed {
                        version: label_version(&row.version),
                    });
                }
            }
            _ => report.findings.push(Finding::UnknownExecution {
                version: label_version(&row.version),
                state: label_state(&row.state),
            }),
        }
    }
    push_absent(&mut report.findings, rows, catalog);
}

fn push_absent(findings: &mut Vec<Finding>, rows: &[JournalRow], catalog: &Catalog) {
    for migration in &catalog.migrations {
        let seen = rows.iter().any(|row| row.version == migration.version);
        if !seen {
            findings.push(Finding::Pending {
                version: label_version(&migration.version),
            });
        }
    }
}

fn push_pending(findings: &mut Vec<Finding>, catalog: &Catalog) {
    for migration in &catalog.migrations {
        findings.push(Finding::Pending {
            version: label_version(&migration.version),
        });
    }
}

fn push_state_without_catalog(findings: &mut Vec<Finding>, row: &JournalRow) {
    match row.state.as_str() {
        "applied" => {}
        "failed" => findings.push(Finding::Failed {
            version: label_version(&row.version),
        }),
        _ => findings.push(Finding::UnknownExecution {
            version: label_version(&row.version),
            state: label_state(&row.state),
        }),
    }
}

fn push_catalog_error(report: &mut DoctorReport, kind: ErrorKind) {
    let finding = match kind {
        ErrorKind::InvalidVersion { version } => Finding::InvalidVersion {
            version: label_version(&version),
        },
        ErrorKind::DuplicateVersion { version } => Finding::DuplicateVersion {
            version: label_version(&version),
        },
        ErrorKind::EmptySql { version } => Finding::EmptySql {
            version: label_version(&version),
        },
        ErrorKind::InvalidSql { version, .. } => Finding::InvalidSql {
            version: label_version(&version),
        },
        ErrorKind::UnsafeSql { version, keyword } => Finding::UnsafeSql {
            version: label_version(&version),
            keyword: label_keyword(&keyword),
        },
        _ => return,
    };
    report.findings.push(finding);
}

fn journal_head(row: &JournalRow) -> JournalHead {
    JournalHead {
        version: label_version(&row.version),
        state: label_state(&row.state),
        checksum: label_checksum(&row.checksum),
    }
}

fn open_is_not_a_database(kind: &ErrorKind) -> bool {
    match kind {
        ErrorKind::Open(source) => {
            source.sqlite_error_code() == Some(rusqlite::ErrorCode::NotADatabase)
        }
        _ => false,
    }
}

fn shown(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("omitted")
}

fn file_label(file: FileStatus) -> &'static str {
    match file {
        FileStatus::Missing => "missing",
        FileStatus::Symlink => "symlink",
        FileStatus::Directory => "directory",
        FileStatus::Present => "present",
        FileStatus::Unreadable => "unreadable",
    }
}

fn app_label(value: &str) -> Option<String> {
    if value.is_empty() || value.len() > 128 {
        return None;
    }
    for character in value.chars() {
        if character.is_control() || character == '\u{2028}' || character == '\u{2029}' {
            return None;
        }
    }
    Some(value.to_owned())
}

fn label_version(value: &str) -> Option<String> {
    if value.len() == 20 && value.bytes().all(|byte| byte.is_ascii_digit()) {
        Some(value.to_owned())
    } else {
        None
    }
}

fn label_state(value: &str) -> Option<String> {
    if (1..=32).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_lowercase()) {
        Some(value.to_owned())
    } else {
        None
    }
}

fn label_checksum(value: &str) -> Option<String> {
    let hex = value.strip_prefix("sha256:raw:v1:")?;
    if hex.len() == 64
        && hex
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        Some(value.to_owned())
    } else {
        None
    }
}

fn label_keyword(value: &str) -> Option<String> {
    match value {
        "begin" | "commit" | "rollback" | "end" | "savepoint" | "release" | "attach" | "detach"
        | "vacuum" | "nul" => Some(value.to_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use rusqlite::OpenFlags;

    use super::{DoctorReport, FileStatus, Finding, doctor};
    use crate::{Database, ErrorKind, Migration};

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    const VERSION_ONE: &str = "20261115000000000001";
    const VERSION_TWO: &str = "20261115000000000002";
    const ROW_SENTINEL: &str = "ROW_SENTINEL_VALUE";
    const SQL_SENTINEL: &str = "SQL_SENTINEL_VALUE";
    const TABLE_SENTINEL: &str = "kept_sentinel_table";

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
                "dbm-sqlite-doctor-{}-{}-{}",
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

    fn applied_sql() -> String {
        format!(
            "CREATE TABLE {TABLE_SENTINEL} (id INTEGER PRIMARY KEY, secret TEXT); INSERT INTO {TABLE_SENTINEL} (id, secret) VALUES (1, '{ROW_SENTINEL}');"
        )
    }

    fn assert_redacted(report: &DoctorReport, forbidden: &[&str]) {
        let text = report.to_string();
        let debug = format!("{report:?}");
        for secret in forbidden {
            assert!(!text.contains(secret), "{text}");
            assert!(!debug.contains(secret), "{debug}");
        }
    }

    fn journal_state(path: &Path, version: &str) -> String {
        let connection = rusqlite::Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .expect("open journal");
        connection
            .query_row(
                "SELECT state FROM dbm_sqlite_journal WHERE version = ?1",
                [version],
                |row| row.get(0),
            )
            .expect("read journal state")
    }

    #[test]
    fn doctor_reports_the_journal_head_without_user_rows_or_sql() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        let sql = applied_sql();
        let migrations = [migration(VERSION_ONE, &sql)];
        drop(Database::open(&path, &migrations).expect("apply"));
        let before = fs::read(&path).expect("read applied file");

        let report = doctor(&path, "1.2.3", &migrations);

        assert_eq!(report.library_version(), "0.1.0");
        assert_eq!(report.sqlite_version(), rusqlite::version());
        assert_eq!(report.app_version(), Some("1.2.3"));
        assert_eq!(report.file(), FileStatus::Present);
        assert_eq!(report.other_tables(), 1);
        assert_eq!(report.journal_rows(), 1);
        let head = report.head().expect("journal head");
        assert_eq!(head.version(), Some(VERSION_ONE));
        assert_eq!(head.state(), Some("applied"));
        assert!(
            head.checksum()
                .is_some_and(|checksum| { checksum.starts_with("sha256:raw:v1:") })
        );
        assert!(report.findings().is_empty());
        let text = report.to_string();
        assert!(text.contains("dbm-sqlite 0.1.0"));
        assert!(text.contains(&format!("sqlite {}", rusqlite::version())));
        assert!(text.contains("app 1.2.3"));
        assert!(text.contains("file present"));
        assert!(text.contains("finding none"));
        assert!(text.contains(head.checksum().expect("checksum")));
        assert_redacted(&report, &[ROW_SENTINEL, SQL_SENTINEL, TABLE_SENTINEL]);
        assert!(!text.contains("CREATE TABLE"));
        assert!(!text.contains(&path.display().to_string()));
        assert_eq!(fs::read(&path).expect("read preserved file"), before);
    }

    #[test]
    fn doctor_does_not_apply_pending_sql() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        let first = [migration(
            VERSION_ONE,
            "CREATE TABLE note (id INTEGER PRIMARY KEY);",
        )];
        drop(Database::open(&path, &first).expect("apply first"));
        let before = fs::read(&path).expect("read applied file");
        let pending_name = "pending_sentinel_table";
        let pending_sql = format!("CREATE TABLE {pending_name} (id INTEGER PRIMARY KEY);");
        let migrations = [first[0], migration(VERSION_TWO, &pending_sql)];

        let report = doctor(&path, "1.2.3", &migrations);

        assert_eq!(report.journal_rows(), 1);
        assert_eq!(
            report.head().and_then(super::JournalHead::version),
            Some(VERSION_ONE)
        );
        assert!(report.findings().iter().any(|finding| matches!(
            finding,
            Finding::Pending { version: Some(version) } if version == VERSION_TWO
        )));
        assert_redacted(&report, &[pending_name]);
        assert_eq!(fs::read(&path).expect("read preserved file"), before);
        let connection = rusqlite::Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .expect("reopen");
        let names: Vec<String> = {
            let mut statement = connection
                .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
                .expect("prepare");
            let mut rows = statement.query([]).expect("query");
            let mut collected = Vec::new();
            while let Some(row) = rows.next().expect("row") {
                collected.push(row.get(0).expect("name"));
            }
            collected
        };
        assert!(!names.iter().any(|name| name == pending_name));
    }

    #[test]
    fn checksum_mismatch_is_visible_and_the_file_stays() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        let original = "CREATE TABLE note (id INTEGER PRIMARY KEY);";
        drop(Database::open(&path, &[migration(VERSION_ONE, original)]).expect("apply"));
        let before = fs::read(&path).expect("read applied file");
        let edited = format!("{original} -- {SQL_SENTINEL}");
        let report = doctor(&path, "1.2.3", &[migration(VERSION_ONE, &edited)]);

        assert!(report.findings().iter().any(|finding| matches!(
            finding,
            Finding::ChecksumMismatch {
                version: Some(version),
                applied_checksum: Some(applied),
                provided_checksum: Some(provided),
            } if version == VERSION_ONE && applied != provided && applied.starts_with("sha256:raw:v1:")
        )));
        let text = report.to_string();
        assert!(text.contains("checksum mismatch"));
        assert!(text.contains("applied sha256:raw:v1:"));
        assert!(text.contains("provided sha256:raw:v1:"));
        assert_redacted(&report, &[SQL_SENTINEL]);
        assert_eq!(fs::read(&path).expect("read preserved file"), before);
    }

    #[test]
    fn failed_execution_is_reported_and_not_retried() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        let migrations = [migration(
            VERSION_ONE,
            "INSERT INTO missing (id) VALUES (1);",
        )];
        let error = Database::open(&path, &migrations).expect_err("migration fails");
        assert!(matches!(error.kind(), ErrorKind::Migration { .. }));
        let before = fs::read(&path).expect("read failed file");

        let report = doctor(&path, "1.2.3", &migrations);

        assert_eq!(
            report.head().and_then(super::JournalHead::state),
            Some("failed")
        );
        assert!(report.findings().iter().any(|finding| matches!(
            finding,
            Finding::Failed { version: Some(version) } if version == VERSION_ONE
        )));
        assert_eq!(journal_state(&path, VERSION_ONE), "failed");
        assert_eq!(fs::read(&path).expect("read preserved file"), before);
    }

    #[test]
    fn an_earlier_unknown_execution_is_reported_with_the_later_head() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        let migrations = [
            migration(VERSION_ONE, "CREATE TABLE note (id INTEGER PRIMARY KEY);"),
            migration(VERSION_TWO, "INSERT INTO note (id) VALUES (1);"),
        ];
        let database = Database::open(&path, &migrations).expect("apply both");
        database
            .connection()
            .expect("owner")
            .execute(
                "UPDATE dbm_sqlite_journal SET state = 'started' WHERE version = ?1",
                [VERSION_ONE],
            )
            .expect("mark started");
        drop(database);
        let before = fs::read(&path).expect("read started file");

        let report = doctor(&path, "1.2.3", &migrations);

        assert_eq!(
            report.head().and_then(super::JournalHead::version),
            Some(VERSION_TWO)
        );
        assert_eq!(
            report.head().and_then(super::JournalHead::state),
            Some("applied")
        );
        assert!(report.findings().iter().any(|finding| matches!(
            finding,
            Finding::UnknownExecution {
                version: Some(version),
                state: Some(state),
            } if version == VERSION_ONE && state == "started"
        )));
        assert_eq!(fs::read(&path).expect("read preserved file"), before);
        assert_eq!(journal_state(&path, VERSION_ONE), "started");
    }

    #[test]
    fn missing_file_is_not_created() {
        let scratch = Scratch::new();
        let path = scratch.path.join("PATH_SENTINEL").join("app.sqlite");
        let report = doctor(
            &path,
            "1.2.3",
            &[migration(
                VERSION_ONE,
                "CREATE TABLE note (id INTEGER PRIMARY KEY);",
            )],
        );

        assert_eq!(report.file(), FileStatus::Missing);
        assert!(report.head().is_none());
        assert!(report.findings().iter().any(|finding| matches!(
            finding,
            Finding::Pending { version: Some(version) } if version == VERSION_ONE
        )));
        assert_redacted(&report, &["PATH_SENTINEL"]);
        assert!(!scratch.path.join("PATH_SENTINEL").exists());
    }

    #[test]
    fn hostile_catalog_text_is_omitted_and_does_not_create_a_file() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        let report = doctor(
            &path,
            "1.0\nROW_SENTINEL_VALUE",
            &[migration(ROW_SENTINEL, "SELECT 1; -- SQL_SENTINEL_VALUE")],
        );

        assert_eq!(report.file(), FileStatus::Missing);
        assert!(report.app_version().is_none());
        assert!(matches!(
            report.findings(),
            [Finding::InvalidVersion { version: None }]
        ));
        assert!(report.to_string().contains("app omitted"));
        assert!(report.to_string().contains("invalid version omitted"));
        assert_redacted(&report, &[ROW_SENTINEL, SQL_SENTINEL]);
        assert!(!path.exists());

        let unsafe_sql = doctor(
            &path,
            "1.2.3",
            &[migration(VERSION_ONE, "COMMIT; -- SQL_SENTINEL_VALUE")],
        );
        assert!(unsafe_sql.findings().iter().any(|finding| matches!(
            finding,
            Finding::UnsafeSql {
                version: Some(version),
                keyword: Some(keyword),
            } if version == VERSION_ONE && keyword == "commit"
        )));
        assert_redacted(&unsafe_sql, &[SQL_SENTINEL]);
        assert!(!path.exists());
    }

    #[test]
    fn tampered_journal_values_and_user_rows_stay_out_of_the_report() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        let connection = rusqlite::Connection::open(&path).expect("create tampered");
        connection
            .execute_batch(
                "CREATE TABLE secret_sentinel_table (secret TEXT);
                 INSERT INTO secret_sentinel_table (secret) VALUES ('ROW_SENTINEL_VALUE');
                 CREATE TABLE dbm_sqlite_journal (version TEXT PRIMARY KEY, checksum TEXT, state TEXT);
                 INSERT INTO dbm_sqlite_journal (version, checksum, state)
                 VALUES ('20261115000000000001', 'ROW_SENTINEL_VALUE', 'paused');",
            )
            .expect("seed tampered journal");
        drop(connection);
        let before = fs::read(&path).expect("read tampered file");

        let report = doctor(&path, "1.2.3", &[migration(VERSION_ONE, "SELECT 1;")]);

        assert_eq!(report.other_tables(), 1);
        assert_eq!(report.journal_rows(), 1);
        let head = report.head().expect("head");
        assert_eq!(head.version(), Some(VERSION_ONE));
        assert_eq!(head.state(), Some("paused"));
        assert_eq!(head.checksum(), None);
        assert!(report.findings().iter().any(|finding| matches!(
            finding,
            Finding::UnknownExecution {
                version: Some(version),
                state: Some(state),
            } if version == VERSION_ONE && state == "paused"
        )));
        assert_redacted(
            &report,
            &[ROW_SENTINEL, "secret_sentinel_table", "SELECT 1"],
        );
        assert_eq!(fs::read(&path).expect("read preserved file"), before);
    }

    #[test]
    fn foreign_text_and_directory_paths_are_left_in_place() {
        let scratch = Scratch::new();
        let foreign = scratch.path.join("foreign.sqlite");
        let connection = rusqlite::Connection::open(&foreign).expect("create foreign");
        connection
            .execute_batch(
                "CREATE TABLE legacy_sentinel_table (secret TEXT);
                 INSERT INTO legacy_sentinel_table (secret) VALUES ('ROW_SENTINEL_VALUE');",
            )
            .expect("seed foreign");
        drop(connection);
        let foreign_before = fs::read(&foreign).expect("read foreign");
        let foreign_report = doctor(
            &foreign,
            "1.2.3",
            &[migration(VERSION_ONE, "SELECT 1; -- SQL_SENTINEL_VALUE")],
        );
        assert_eq!(foreign_report.file(), FileStatus::Present);
        assert_eq!(foreign_report.other_tables(), 1);
        assert!(foreign_report.head().is_none());
        assert!(matches!(
            foreign_report.findings(),
            [Finding::UnrecognizedFile]
        ));
        assert_redacted(
            &foreign_report,
            &[
                ROW_SENTINEL,
                SQL_SENTINEL,
                "legacy_sentinel_table",
                "SELECT 1",
            ],
        );
        assert_eq!(
            fs::read(&foreign).expect("read preserved foreign"),
            foreign_before
        );

        let text = scratch.path.join("notes.txt");
        fs::write(&text, b"NOT_A_DATABASE_SENTINEL").expect("write text");
        let text_report = doctor(&text, "1.2.3", &[]);
        assert_eq!(text_report.file(), FileStatus::Present);
        assert!(matches!(text_report.findings(), [Finding::NotADatabase]));
        assert_redacted(&text_report, &["NOT_A_DATABASE_SENTINEL"]);
        assert_eq!(
            fs::read(&text).expect("read preserved text"),
            b"NOT_A_DATABASE_SENTINEL"
        );

        let directory = scratch.path.join("DIR_SENTINEL");
        fs::create_dir(&directory).expect("create directory");
        let directory_report = doctor(&directory, "1.2.3", &[]);
        assert_eq!(directory_report.file(), FileStatus::Directory);
        assert_redacted(&directory_report, &["DIR_SENTINEL"]);
    }

    #[test]
    fn an_empty_file_is_not_turned_into_a_database() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        fs::write(&path, b"").expect("write empty");

        let report = doctor(
            &path,
            "1.2.3",
            &[migration(
                VERSION_ONE,
                "CREATE TABLE note (id INTEGER PRIMARY KEY);",
            )],
        );

        assert_eq!(fs::read(&path).expect("read empty"), b"");
        assert_eq!(report.file(), FileStatus::Present);
        assert_eq!(report.journal_rows(), 0);
        assert!(report.head().is_none());
        assert!(report.findings().iter().any(|finding| matches!(
            finding,
            Finding::Pending { version: Some(version) } if version == VERSION_ONE
        )));
        assert!(!report.to_string().contains("CREATE TABLE"));
    }

    #[cfg(unix)]
    #[test]
    fn doctor_does_not_follow_a_symlink() {
        let scratch = Scratch::new();
        let target = scratch.path.join("TARGET_SENTINEL");
        let link = scratch.path.join("link.sqlite");
        fs::write(&target, b"TARGET_SENTINEL_BYTES").expect("write target");
        std::os::unix::fs::symlink(&target, &link).expect("create symlink");

        let report = doctor(&link, "1.2.3", &[]);

        assert_eq!(report.file(), FileStatus::Symlink);
        assert!(report.head().is_none());
        assert_redacted(&report, &["TARGET_SENTINEL"]);
        assert_eq!(
            fs::read(&target).expect("read target"),
            b"TARGET_SENTINEL_BYTES"
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_unreadable_file_is_not_modified() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        let migrations = [migration(
            VERSION_ONE,
            "CREATE TABLE note (id INTEGER PRIMARY KEY);",
        )];
        drop(Database::open(&path, &migrations).expect("apply"));
        let before = fs::read(&path).expect("read applied file");
        struct RestoreMode<'a> {
            path: &'a Path,
        }
        impl Drop for RestoreMode<'_> {
            fn drop(&mut self) {
                let _ = fs::set_permissions(self.path, fs::Permissions::from_mode(0o644));
            }
        }
        let _restore = RestoreMode { path: &path };
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("deny read");
        let denied = fs::File::open(&path).is_err();

        let report = doctor(&path, "1.2.3", &migrations);
        drop(_restore);

        assert_eq!(fs::read(&path).expect("read restored file"), before);
        if denied {
            assert_eq!(report.file(), FileStatus::Unreadable);
            assert!(report.head().is_none());
            assert_redacted(&report, &["CREATE TABLE"]);
        }
    }

    #[test]
    fn missing_applied_migration_is_a_finding() {
        let scratch = Scratch::new();
        let path = scratch.path.join("app.sqlite");
        drop(
            Database::open(
                &path,
                &[migration(VERSION_ONE, "CREATE TABLE note (id INTEGER);")],
            )
            .expect("apply"),
        );
        let before = fs::read(&path).expect("read applied file");

        let report = doctor(&path, "1.2.3", &[]);

        assert!(report.findings().iter().any(|finding| matches!(
            finding,
            Finding::MissingMigration { version: Some(version) } if version == VERSION_ONE
        )));
        assert_eq!(fs::read(&path).expect("read preserved file"), before);
    }
}
