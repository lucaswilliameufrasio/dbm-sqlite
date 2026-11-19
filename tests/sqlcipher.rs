#![cfg(feature = "sqlcipher")]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use dbm_sqlite::{Database, ErrorKind, FileStatus, Finding, Migration, doctor_encrypted};

const V1: Migration<'static> = Migration {
    version: "20261118000000000001",
    sql: "CREATE TABLE note (body TEXT NOT NULL);",
};
const V2: Migration<'static> = Migration {
    version: "20261118000000000002",
    sql: "ALTER TABLE note ADD COLUMN category TEXT NOT NULL DEFAULT 'general';",
};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("dbm-sqlcipher-{}-{nanos}-{id}", std::process::id()));
        fs::create_dir(&path).expect("create isolated test directory");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn encrypted_database_survives_upgrade_and_rejects_wrong_key_without_writing() {
    let scratch = Scratch::new();
    let path = scratch.0.join("encrypted.sqlite");
    let key = "test-only-passphrase-'quoted'";
    let database = Database::open_encrypted(&path, &[V1], key).expect("create encrypted");
    database
        .connection()
        .expect("connection")
        .execute(
            "INSERT INTO note (body) VALUES (?1)",
            ["synthetic-row-sentinel"],
        )
        .expect("insert");
    drop(database);

    let bytes = fs::read(&path).expect("encrypted bytes");
    assert!(!bytes.starts_with(b"SQLite format 3\0"));
    assert!(
        !bytes
            .windows(22)
            .any(|bytes| bytes == b"synthetic-row-sentinel")
    );

    let error = Database::open_encrypted(&path, &[V1, V2], "test-only-wrong")
        .expect_err("wrong key must fail");
    assert!(!error.to_string().contains("test-only-wrong"));
    assert_eq!(fs::read(&path).expect("preserved bytes"), bytes);
    assert!(Database::open(&path, &[V1, V2]).is_err());
    assert_eq!(fs::read(&path).expect("still preserved"), bytes);

    let upgraded = Database::open_encrypted(&path, &[V1, V2], key).expect("upgrade");
    let (body, category): (String, String) = upgraded
        .connection()
        .expect("connection")
        .query_row("SELECT body, category FROM note", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("read upgraded row");
    assert_eq!(body, "synthetic-row-sentinel");
    assert_eq!(category, "general");
    drop(upgraded);

    let reopened = Database::open_encrypted(&path, &[V1, V2], key).expect("reopen");
    assert_eq!(
        reopened
            .connection()
            .expect("connection")
            .query_row("SELECT COUNT(*) FROM note", [], |row| row.get::<_, i64>(0))
            .expect("count"),
        1
    );
}

#[test]
fn invalid_key_never_creates_a_database_or_owner_lock() {
    let scratch = Scratch::new();
    let path = scratch.0.join("missing.sqlite");
    for key in ["", "invalid\0key"] {
        let error = Database::open_encrypted(&path, &[V1], key).expect_err("invalid key");
        assert!(matches!(error.kind(), ErrorKind::InvalidEncryptionKey));
        assert!(!path.exists());
        assert!(!scratch.0.join("missing.sqlite.dbm-owner").exists());
    }
}

#[test]
fn plaintext_file_is_not_converted_or_modified() {
    let scratch = Scratch::new();
    let path = scratch.0.join("plain.sqlite");
    let database = Database::open(&path, &[V1]).expect("create plaintext");
    drop(database);
    let bytes = fs::read(&path).expect("plaintext bytes");
    assert!(bytes.starts_with(b"SQLite format 3\0"));
    assert!(Database::open_encrypted(&path, &[V1, V2], "test-only-key").is_err());
    assert_eq!(fs::read(&path).expect("preserved plaintext"), bytes);
    let database = Database::open(&path, &[V1]).expect("plaintext remains usable");
    drop(database);
}

#[test]
fn encrypted_journal_rejects_edited_migration_without_writing() {
    let scratch = Scratch::new();
    let path = scratch.0.join("journal.sqlite");
    let key = "test-only-journal-key";
    let database = Database::open_encrypted(&path, &[V1], key).expect("create encrypted");
    database
        .connection()
        .expect("connection")
        .execute("INSERT INTO note (body) VALUES (?1)", ["kept"])
        .expect("insert");
    drop(database);
    let before = fs::read(&path).expect("read encrypted bytes");
    let edited = Migration {
        version: V1.version,
        sql: "CREATE TABLE note (body TEXT, extra TEXT);",
    };
    let error = Database::open_encrypted(&path, &[edited], key).expect_err("edited migration");
    assert!(matches!(error.kind(), ErrorKind::ChecksumMismatch { .. }));
    assert!(!format!("{error:?} {error}").contains(key));
    assert_eq!(fs::read(&path).expect("unchanged file"), before);
    let database = Database::open_encrypted(&path, &[V1], key).expect("reopen original");
    let body: String = database
        .connection()
        .expect("connection")
        .query_row("SELECT body FROM note", [], |row| row.get(0))
        .expect("read preserved row");
    assert_eq!(body, "kept");
}

#[test]
fn encrypted_doctor_reports_journal_without_exposing_secrets_or_writing() {
    let scratch = Scratch::new();
    let path = scratch.0.join("private-path-sentinel.sqlite");
    let key = "test-only-private-key-sentinel";
    let database = Database::open_encrypted(&path, &[V1], key).expect("create encrypted");
    database
        .connection()
        .expect("connection")
        .execute(
            "INSERT INTO note (body) VALUES (?1)",
            ["private-row-sentinel"],
        )
        .expect("insert");
    drop(database);
    let before = fs::read(&path).expect("encrypted bytes");

    let report = doctor_encrypted(&path, "1.0.0", &[V1, V2], key).expect("doctor");
    assert_eq!(report.file(), FileStatus::Present);
    assert!(report.findings().iter().any(
        |finding| matches!(finding, Finding::Pending { version: Some(version) } if version == V2.version)
    ));
    let text = format!("{report:?}\n{report}");
    for secret in [key, "private-path-sentinel", "private-row-sentinel", V1.sql] {
        assert!(!text.contains(secret));
    }
    assert_eq!(fs::read(&path).expect("database unchanged"), before);

    let wrong = doctor_encrypted(&path, "1.0.0", &[V1, V2], "test-only-wrong")
        .expect("wrong key yields report");
    assert_eq!(wrong.file(), FileStatus::Present);
    assert!(wrong.findings().contains(&Finding::EncryptedUnreadable));
    assert!(!format!("{wrong:?}\n{wrong}").contains("test-only-wrong"));
    assert_eq!(fs::read(&path).expect("database still unchanged"), before);
}

#[test]
fn encrypted_doctor_does_not_create_files_and_rejects_invalid_keys() {
    let scratch = Scratch::new();
    let path = scratch.0.join("missing.sqlite");
    let report = doctor_encrypted(&path, "1.0.0", &[V1], "test-only-key").expect("doctor");
    assert_eq!(report.file(), FileStatus::Missing);
    assert!(!path.exists());
    assert!(!scratch.0.join("missing.sqlite.dbm-owner").exists());
    for key in ["", "invalid\0key"] {
        let error = doctor_encrypted(&path, "1.0.0", &[V1], key).expect_err("invalid key");
        assert!(matches!(error.kind(), ErrorKind::InvalidEncryptionKey));
        assert!(!path.exists());
    }
}

#[test]
fn encrypted_doctor_reads_live_wal_without_changing_database_or_wal() {
    let scratch = Scratch::new();
    let path = scratch.0.join("wal.sqlite");
    let wal = scratch.0.join("wal.sqlite-wal");
    let key = "test-only-wal-key";
    let database = Database::open_encrypted(&path, &[V1], key).expect("open");
    {
        let connection = database.connection().expect("connection");
        let mode: String = connection
            .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
            .expect("enable WAL");
        assert_eq!(mode, "wal");
        connection
            .execute("INSERT INTO note (body) VALUES (?1)", ["wal-row"])
            .expect("insert");
    }
    let before_db = fs::read(&path).expect("database bytes");
    let before_wal = fs::read(&wal).expect("WAL bytes");
    let report = doctor_encrypted(&path, "1.0.0", &[V1], key).expect("doctor");
    assert_eq!(report.file(), FileStatus::Present);
    assert!(report.findings().is_empty());
    assert_eq!(fs::read(&path).expect("database unchanged"), before_db);
    assert_eq!(fs::read(&wal).expect("WAL unchanged"), before_wal);
    drop(database);
}

#[cfg(unix)]
#[test]
fn encrypted_doctor_refuses_symlink_without_touching_target() {
    let scratch = Scratch::new();
    let target = scratch.0.join("target.sqlite");
    let link = scratch.0.join("link.sqlite");
    let database = Database::open_encrypted(&target, &[V1], "test-only-key").expect("create");
    drop(database);
    let before = fs::read(&target).expect("target");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");
    let report = doctor_encrypted(&link, "1.0.0", &[V1], "test-only-key").expect("doctor");
    assert_eq!(report.file(), FileStatus::Symlink);
    assert_eq!(fs::read(&target).expect("target unchanged"), before);
}
