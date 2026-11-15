use std::fs;
use std::io;
use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior};

use crate::Migration;
use crate::checksum::raw_checksum;
use crate::error::{Error, ErrorKind};
use crate::guard::{self, SqlReject};
use crate::version::validate_version;

const JOURNAL_TABLE: &str = "dbm_sqlite_journal";

const CREATE_JOURNAL: &str = "CREATE TABLE dbm_sqlite_journal (
    version TEXT PRIMARY KEY,
    checksum TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('started', 'applied', 'failed'))
) STRICT";

struct PreparedMigration {
    version: String,
    sql: String,
    checksum: String,
}

struct Catalog {
    migrations: Vec<PreparedMigration>,
}

struct JournalRow {
    version: String,
    checksum: String,
    state: String,
}

#[derive(Debug)]
struct PendingMigration {
    version: String,
    sql: String,
    checksum: String,
    retry: bool,
}

enum ExistingFile {
    Idle,
    Pending,
}

pub(crate) fn open_database(
    path: &Path,
    migrations: &[Migration<'_>],
) -> Result<Connection, Error> {
    let catalog = prepare(migrations).map_err(|kind| Error::new(path, kind))?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(Error::new(path, ErrorKind::Symlink))
        }
        Ok(metadata) if metadata.is_dir() => Err(Error::new(path, ErrorKind::Directory)),
        Ok(_) => open_existing(path, &catalog),
        Err(source) if source.kind() == io::ErrorKind::NotFound => open_new(path, &catalog),
        Err(source) => Err(Error::new(path, ErrorKind::Io(source))),
    }
}

fn open_new(path: &Path, catalog: &Catalog) -> Result<Connection, Error> {
    ensure_parent_exists(path).map_err(|kind| Error::new(path, kind))?;
    let mut connection = match open_writable(path) {
        Ok(connection) => connection,
        Err(kind) => {
            let _ = fs::remove_file(path);
            return Err(Error::new(path, kind));
        }
    };
    match apply(&mut connection, catalog) {
        Ok(()) => Ok(connection),
        Err(kind) => {
            let keep = journal_has_row(&connection);
            drop(connection);
            if !keep {
                let _ = fs::remove_file(path);
            }
            Err(Error::new(path, kind))
        }
    }
}

fn open_existing(path: &Path, catalog: &Catalog) -> Result<Connection, Error> {
    let existing = inspect(path, catalog).map_err(|kind| Error::new(path, kind))?;
    let mut connection = open_writable(path).map_err(|kind| Error::new(path, kind))?;
    if matches!(existing, ExistingFile::Pending) {
        apply(&mut connection, catalog).map_err(|kind| Error::new(path, kind))?;
    }
    Ok(connection)
}

fn inspect(path: &Path, catalog: &Catalog) -> Result<ExistingFile, ErrorKind> {
    let connection = open_readonly(path)?;
    let names = table_names(&connection)?;
    let has_journal = names.iter().any(|name| name == JOURNAL_TABLE);
    if !has_journal {
        if names.is_empty() {
            return Ok(ExistingFile::Pending);
        }
        return Err(ErrorKind::UnrecognizedFile);
    }
    let rows = read_journal(&connection)?;
    let pending = decide(&rows, catalog)?;
    if pending.is_empty() {
        Ok(ExistingFile::Idle)
    } else {
        Ok(ExistingFile::Pending)
    }
}

fn apply(connection: &mut Connection, catalog: &Catalog) -> Result<(), ErrorKind> {
    loop {
        let pending = claim_next(connection, catalog)?;
        let Some(pending) = pending else {
            return Ok(());
        };
        run_pending(connection, &pending)?;
    }
}

fn claim_next(
    connection: &mut Connection,
    catalog: &Catalog,
) -> Result<Option<PendingMigration>, ErrorKind> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let names = table_names(&transaction)?;
    let has_journal = names.iter().any(|name| name == JOURNAL_TABLE);
    let mut created_journal = false;
    if !has_journal {
        if !names.is_empty() {
            transaction.rollback()?;
            return Err(ErrorKind::UnrecognizedFile);
        }
        transaction.execute_batch(CREATE_JOURNAL)?;
        created_journal = true;
    }
    let rows = read_journal(&transaction)?;
    let mut pending = decide(&rows, catalog)?;
    if pending.is_empty() {
        if created_journal {
            transaction.commit()?;
        } else {
            transaction.rollback()?;
        }
        return Ok(None);
    }
    let next = pending.remove(0);
    mark_started(&transaction, &next)?;
    transaction.commit()?;
    Ok(Some(next))
}

fn mark_started(connection: &Connection, pending: &PendingMigration) -> Result<(), ErrorKind> {
    let changed = if pending.retry {
        connection.execute(
            "UPDATE dbm_sqlite_journal SET checksum = ?1, state = 'started' WHERE version = ?2 AND state = 'failed'",
            (&pending.checksum, &pending.version),
        )?
    } else {
        connection.execute(
            "INSERT INTO dbm_sqlite_journal (version, checksum, state) VALUES (?1, ?2, 'started')",
            (&pending.version, &pending.checksum),
        )?
    };
    if changed != 1 {
        return Err(ErrorKind::JournalConflict {
            version: pending.version.clone(),
        });
    }
    Ok(())
}

fn run_pending(connection: &mut Connection, pending: &PendingMigration) -> Result<(), ErrorKind> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    match transaction.execute_batch(&pending.sql) {
        Ok(()) => {
            let changed = transaction.execute(
                "UPDATE dbm_sqlite_journal SET state = 'applied' WHERE version = ?1 AND state = 'started'",
                [&pending.version],
            )?;
            if changed != 1 {
                transaction.rollback()?;
                return Err(ErrorKind::UnknownExecution {
                    version: pending.version.clone(),
                    state: current_state(connection, &pending.version)?,
                });
            }
            match transaction.commit() {
                Ok(()) => Ok(()),
                Err(source) => Err(ErrorKind::Migration {
                    version: pending.version.clone(),
                    source,
                }),
            }
        }
        Err(source) => {
            transaction.rollback()?;
            record_failed(connection, &pending.version)?;
            Err(ErrorKind::Migration {
                version: pending.version.clone(),
                source,
            })
        }
    }
}

fn record_failed(connection: &mut Connection, version: &str) -> Result<(), ErrorKind> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let changed = transaction.execute(
        "UPDATE dbm_sqlite_journal SET state = 'failed' WHERE version = ?1 AND state = 'started'",
        [version],
    )?;
    if changed != 1 {
        transaction.rollback()?;
        return Err(ErrorKind::UnknownExecution {
            version: version.to_owned(),
            state: current_state(connection, version)?,
        });
    }
    transaction.commit()?;
    Ok(())
}

fn current_state(connection: &Connection, version: &str) -> Result<String, rusqlite::Error> {
    let state = connection
        .query_row(
            "SELECT state FROM dbm_sqlite_journal WHERE version = ?1",
            [version],
            |row| row.get(0),
        )
        .optional()?;
    Ok(state.unwrap_or_else(|| "absent".to_owned()))
}

fn prepare(migrations: &[Migration<'_>]) -> Result<Catalog, ErrorKind> {
    let mut prepared = Vec::with_capacity(migrations.len());
    for migration in migrations {
        if !validate_version(migration.version) {
            return Err(ErrorKind::InvalidVersion {
                version: migration.version.to_owned(),
            });
        }
        match guard::check_migration_sql(migration.sql) {
            Ok(()) => {}
            Err(SqlReject::Empty) => {
                return Err(ErrorKind::EmptySql {
                    version: migration.version.to_owned(),
                });
            }
            Err(SqlReject::Unterminated) => {
                return Err(ErrorKind::InvalidSql {
                    version: migration.version.to_owned(),
                    reason: "unterminated SQL".to_owned(),
                });
            }
            Err(SqlReject::Forbidden { keyword }) => {
                return Err(ErrorKind::UnsafeSql {
                    version: migration.version.to_owned(),
                    keyword,
                });
            }
        }
        prepared.push(PreparedMigration {
            version: migration.version.to_owned(),
            sql: migration.sql.to_owned(),
            checksum: raw_checksum(migration.sql.as_bytes()),
        });
    }
    prepared.sort_by(|left, right| left.version.cmp(&right.version));

    let mut previous: Option<&str> = None;
    for migration in &prepared {
        if let Some(prior) = previous
            && prior == migration.version
        {
            return Err(ErrorKind::DuplicateVersion {
                version: migration.version.clone(),
            });
        }
        previous = Some(migration.version.as_str());
    }
    Ok(Catalog {
        migrations: prepared,
    })
}

fn decide(rows: &[JournalRow], catalog: &Catalog) -> Result<Vec<PendingMigration>, ErrorKind> {
    let mut applied = Vec::new();
    let mut retries = Vec::new();
    for row in rows {
        match row.state.as_str() {
            "started" => {
                return Err(ErrorKind::UnknownExecution {
                    version: row.version.clone(),
                    state: row.state.clone(),
                });
            }
            "applied" => {
                let Some(migration) = find_migration(catalog, &row.version) else {
                    return Err(ErrorKind::MissingMigration {
                        version: row.version.clone(),
                    });
                };
                if migration.checksum != row.checksum {
                    return Err(ErrorKind::ChecksumMismatch {
                        version: row.version.clone(),
                        applied_checksum: row.checksum.clone(),
                        provided_checksum: migration.checksum.clone(),
                    });
                }
                applied.push(row.version.clone());
            }
            "failed" => {
                if find_migration(catalog, &row.version).is_none() {
                    return Err(ErrorKind::UnresolvedFailure {
                        version: row.version.clone(),
                    });
                }
                retries.push(row.version.clone());
            }
            _ => {
                return Err(ErrorKind::UnknownExecution {
                    version: row.version.clone(),
                    state: row.state.clone(),
                });
            }
        }
    }

    let mut pending = Vec::new();
    for migration in &catalog.migrations {
        let already_applied = applied.iter().any(|version| version == &migration.version);
        if already_applied {
            continue;
        }
        let retry = retries.iter().any(|version| version == &migration.version);
        pending.push(PendingMigration {
            version: migration.version.clone(),
            sql: migration.sql.clone(),
            checksum: migration.checksum.clone(),
            retry,
        });
    }
    Ok(pending)
}

fn find_migration<'a>(catalog: &'a Catalog, version: &str) -> Option<&'a PreparedMigration> {
    catalog
        .migrations
        .iter()
        .find(|migration| migration.version == version)
}

fn ensure_parent_exists(path: &Path) -> Result<(), ErrorKind> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() {
        return Ok(());
    }
    match parent.metadata() {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(ErrorKind::Io(io::Error::new(
            io::ErrorKind::NotADirectory,
            "parent path is not a directory",
        ))),
        Err(source) => Err(ErrorKind::Io(source)),
    }
}

fn open_writable(path: &Path) -> Result<Connection, ErrorKind> {
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
        | OpenFlags::SQLITE_OPEN_CREATE
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_NOFOLLOW;
    match Connection::open_with_flags(path, flags) {
        Ok(connection) => Ok(connection),
        Err(source) => Err(ErrorKind::Open(source)),
    }
}

fn open_readonly(path: &Path) -> Result<Connection, ErrorKind> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW;
    match Connection::open_with_flags(path, flags) {
        Ok(connection) => Ok(connection),
        Err(source) => Err(ErrorKind::Open(source)),
    }
}

fn table_names(connection: &Connection) -> Result<Vec<String>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let mut rows = statement.query([])?;
    let mut names = Vec::new();
    while let Some(row) = rows.next()? {
        names.push(row.get(0)?);
    }
    Ok(names)
}

fn read_journal(connection: &Connection) -> Result<Vec<JournalRow>, rusqlite::Error> {
    let mut statement = connection
        .prepare("SELECT version, checksum, state FROM dbm_sqlite_journal ORDER BY version")?;
    let mut rows = statement.query([])?;
    let mut journal = Vec::new();
    while let Some(row) = rows.next()? {
        journal.push(JournalRow {
            version: row.get(0)?,
            checksum: row.get(1)?,
            state: row.get(2)?,
        });
    }
    Ok(journal)
}

fn journal_has_row(connection: &Connection) -> bool {
    let count = connection.query_row("SELECT COUNT(*) FROM dbm_sqlite_journal", [], |row| {
        row.get::<_, i64>(0)
    });
    matches!(count, Ok(count) if count > 0)
}

#[cfg(test)]
mod tests {
    use super::decide;
    use super::{Catalog, JournalRow, PreparedMigration};
    use crate::error::ErrorKind;

    fn migration(version: &str, checksum: &str) -> PreparedMigration {
        PreparedMigration {
            version: version.to_owned(),
            sql: "SELECT 1".to_owned(),
            checksum: checksum.to_owned(),
        }
    }

    fn row(version: &str, checksum: &str, state: &str) -> JournalRow {
        JournalRow {
            version: version.to_owned(),
            checksum: checksum.to_owned(),
            state: state.to_owned(),
        }
    }

    #[test]
    fn applied_checksum_mismatch_is_visible_before_any_pending_work() {
        let catalog = Catalog {
            migrations: vec![
                migration("20261115000000000001", "sha256:raw:v1:one"),
                migration("20261115000000000002", "sha256:raw:v1:two"),
            ],
        };
        let rows = vec![row(
            "20261115000000000001",
            "sha256:raw:v1:other",
            "applied",
        )];
        let decision = decide(&rows, &catalog).unwrap_err();
        match decision {
            ErrorKind::ChecksumMismatch {
                version,
                applied_checksum,
                provided_checksum,
            } => {
                assert_eq!(version, "20261115000000000001");
                assert_eq!(applied_checksum, "sha256:raw:v1:other");
                assert_eq!(provided_checksum, "sha256:raw:v1:one");
            }
            other => panic!("expected checksum mismatch, got {other:?}"),
        }
    }

    #[test]
    fn started_and_unknown_states_block_before_retrying() {
        let catalog = Catalog {
            migrations: vec![migration("20261115000000000001", "sha256:raw:v1:one")],
        };
        let started = decide(
            &[row("20261115000000000001", "sha256:raw:v1:one", "started")],
            &catalog,
        )
        .unwrap_err();
        assert!(matches!(
            started,
            ErrorKind::UnknownExecution { ref state, .. } if state == "started"
        ));

        let unknown = decide(
            &[row("20261115000000000001", "sha256:raw:v1:one", "paused")],
            &catalog,
        )
        .unwrap_err();
        assert!(matches!(
            unknown,
            ErrorKind::UnknownExecution { ref state, .. } if state == "paused"
        ));
    }
}
