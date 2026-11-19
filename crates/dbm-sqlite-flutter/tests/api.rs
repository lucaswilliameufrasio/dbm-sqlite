use std::ffi::{CStr, CString};
use std::fs;
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use dbm_sqlite_flutter::{
    DBM_SQLITE_CHECKSUM_MISMATCH, DBM_SQLITE_INVALID, DBM_SQLITE_OK, DBM_SQLITE_OWNED,
    DBM_SQLITE_UNKNOWN_EXECUTION, dbm_sqlite_close, dbm_sqlite_doctor, dbm_sqlite_execute,
    dbm_sqlite_free, dbm_sqlite_open, dbm_sqlite_query,
};

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "dbm-sqlite-flutter-{}-{}-{}",
            std::process::id(),
            n,
            nanos
        ));
        fs::create_dir(&path).expect("scratch");
        Self { path }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[derive(Debug)]
struct Opened {
    handle: u64,
}

impl Drop for Opened {
    fn drop(&mut self) {
        if self.handle == 0 {
            return;
        }
        let mut err = [0u8; 64];
        unsafe {
            dbm_sqlite_close(self.handle, err.as_mut_ptr().cast(), err.len());
        }
        self.handle = 0;
    }
}

fn c_string(text: &str) -> CString {
    CString::new(text).expect("c string")
}

fn migration_blob(version: &str, sql: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&(version.len() as u32).to_le_bytes());
    bytes.extend_from_slice(version.as_bytes());
    bytes.extend_from_slice(&(sql.len() as u32).to_le_bytes());
    bytes.extend_from_slice(sql.as_bytes());
    bytes
}

fn empty_args() -> Vec<u8> {
    0u32.to_le_bytes().to_vec()
}

fn read_err(err: &[u8]) -> String {
    let len = err.iter().position(|byte| *byte == 0).unwrap_or(err.len());
    String::from_utf8_lossy(&err[..len]).into_owned()
}

fn open_at(path: &str, version: &str, sql: &str) -> Result<Opened, (i32, String)> {
    let blob = migration_blob(version, sql);
    let path = c_string(path);
    let mut handle = 0u64;
    let mut err = [0u8; 1024];
    let code = unsafe {
        dbm_sqlite_open(
            path.as_ptr(),
            blob.as_ptr(),
            blob.len(),
            &mut handle,
            err.as_mut_ptr().cast(),
            err.len(),
        )
    };
    if code != DBM_SQLITE_OK {
        return Err((code, read_err(&err)));
    }
    Ok(Opened { handle })
}

fn execute(handle: u64, sql: &str) -> Result<i64, (i32, String)> {
    let sql = c_string(sql);
    let args = empty_args();
    let mut changes = 0i64;
    let mut err = [0u8; 1024];
    let code = unsafe {
        dbm_sqlite_execute(
            handle,
            sql.as_ptr(),
            args.as_ptr(),
            args.len(),
            &mut changes,
            err.as_mut_ptr().cast(),
            err.len(),
        )
    };
    if code != DBM_SQLITE_OK {
        return Err((code, read_err(&err)));
    }
    Ok(changes)
}

fn query_json(handle: u64, sql: &str) -> Result<String, (i32, String)> {
    let sql = c_string(sql);
    let args = empty_args();
    let mut out: *mut c_char = std::ptr::null_mut();
    let mut err = [0u8; 1024];
    let code = unsafe {
        dbm_sqlite_query(
            handle,
            sql.as_ptr(),
            args.as_ptr(),
            args.len(),
            &mut out,
            err.as_mut_ptr().cast(),
            err.len(),
        )
    };
    if code != DBM_SQLITE_OK {
        return Err((code, read_err(&err)));
    }
    assert!(!out.is_null());
    let text = unsafe { CStr::from_ptr(out) }
        .to_str()
        .expect("json utf-8")
        .to_string();
    unsafe {
        dbm_sqlite_free(out);
    }
    Ok(text)
}

const VERSION: &str = "20261115000000000001";
const CREATE: &str = "CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT);";

#[cfg(feature = "sqlcipher")]
#[test]
fn encrypted_abi_preserves_data_and_reports_without_secrets() {
    use dbm_sqlite_flutter::{dbm_sqlite_doctor_encrypted, dbm_sqlite_open_encrypted};

    let scratch = Scratch::new();
    let path = scratch.path.join("private-name.sqlite");
    let c_path = c_string(path.to_str().expect("path"));
    let app = c_string("1.0.0");
    let key = b"test-only-secret";
    let blob = migration_blob(VERSION, CREATE);
    let mut handle = 99u64;
    let mut err = [0u8; 1024];
    let open = |key: &[u8], handle: &mut u64, err: &mut [u8]| unsafe {
        dbm_sqlite_open_encrypted(
            c_path.as_ptr(),
            key.as_ptr(),
            key.len(),
            blob.as_ptr(),
            blob.len(),
            handle,
            err.as_mut_ptr().cast(),
            err.len(),
        )
    };

    assert_eq!(open(b"", &mut handle, &mut err), DBM_SQLITE_INVALID);
    assert_eq!(handle, 0);
    assert!(!path.exists());
    assert_eq!(open(&[0xff], &mut handle, &mut err), DBM_SQLITE_INVALID);
    assert_eq!(handle, 0);
    assert_eq!(open(key, &mut handle, &mut err), DBM_SQLITE_OK);
    execute(handle, "INSERT INTO note (body) VALUES ('private-row');").expect("insert");
    drop(Opened { handle });
    let original = fs::read(&path).expect("read encrypted bytes");
    assert!(!original.starts_with(b"SQLite format 3\0"));

    handle = 99;
    assert_ne!(
        open(b"test-only-wrong", &mut handle, &mut err),
        DBM_SQLITE_OK
    );
    assert_eq!(handle, 0);
    assert!(!read_err(&err).contains("test-only-wrong"));
    assert_eq!(fs::read(&path).expect("preserved"), original);

    let mut text: *mut c_char = std::ptr::null_mut();
    let status = unsafe {
        dbm_sqlite_doctor_encrypted(
            c_path.as_ptr(),
            app.as_ptr(),
            key.as_ptr(),
            key.len(),
            blob.as_ptr(),
            blob.len(),
            &mut text,
            err.as_mut_ptr().cast(),
            err.len(),
        )
    };
    assert_eq!(status, DBM_SQLITE_OK);
    let report = unsafe { CStr::from_ptr(text) }.to_str().expect("report");
    assert!(report.contains("applied"));
    for secret in ["test-only-secret", "private-name", "private-row", CREATE] {
        assert!(!report.contains(secret));
    }
    unsafe { dbm_sqlite_free(text) };
    assert_eq!(fs::read(&path).expect("doctor is read-only"), original);

    assert_eq!(open(key, &mut handle, &mut err), DBM_SQLITE_OK);
    let owner = Opened { handle };
    assert!(
        query_json(owner.handle, "SELECT body FROM note")
            .expect("read")
            .contains("private-row")
    );
}

#[test]
fn open_applies_caller_sql_and_a_second_open_does_not_write() {
    let scratch = Scratch::new();
    let path = scratch.path.join("app.sqlite");
    let path = path.to_str().expect("unicode");
    let opened = open_at(path, VERSION, CREATE).expect("open");
    assert_eq!(
        execute(
            opened.handle,
            "INSERT INTO note (body) VALUES ('primeira');"
        )
        .expect("insert"),
        1
    );
    let json = query_json(opened.handle, "SELECT body FROM note;").expect("query");
    assert!(json.contains("primeira"));

    let again = open_at(path, VERSION, CREATE).expect_err("second open");
    assert_eq!(again.0, DBM_SQLITE_OWNED);
    assert!(again.1.contains("already has a connection owner"));
    assert!(again.1.contains("did not write"));
}

#[test]
fn editing_sql_does_not_require_another_library() {
    let scratch = Scratch::new();
    let first = scratch.path.join("one.sqlite");
    let second = scratch.path.join("two.sqlite");
    let first = first.to_str().expect("unicode");
    let second = second.to_str().expect("unicode");
    let left = open_at(first, VERSION, CREATE).expect("first sql");
    let right = open_at(
        second,
        VERSION,
        "CREATE TABLE other (id INTEGER PRIMARY KEY);",
    )
    .expect("second sql");
    assert!(query_json(left.handle, "SELECT COUNT(*) FROM note;").is_ok());
    assert!(query_json(right.handle, "SELECT COUNT(*) FROM other;").is_ok());
}

#[test]
fn checksum_mismatch_keeps_the_file() {
    let scratch = Scratch::new();
    let path = scratch.path.join("app.sqlite");
    let path = path.to_str().expect("unicode");
    {
        let opened = open_at(path, VERSION, CREATE).expect("apply");
        drop(opened);
    }
    let before = fs::read(path).expect("read");
    let edited = open_at(
        path,
        VERSION,
        "CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT, extra TEXT);",
    );
    let error = edited.expect_err("mismatch");
    assert_eq!(error.0, DBM_SQLITE_CHECKSUM_MISMATCH);
    assert!(error.1.contains("checksum mismatch"));
    assert_eq!(fs::read(path).expect("preserved"), before);
}

#[test]
fn unknown_execution_blocks_the_next_open() {
    let scratch = Scratch::new();
    let path = scratch.path.join("app.sqlite");
    let path = path.to_str().expect("unicode");
    {
        let opened = open_at(path, VERSION, CREATE).expect("apply");
        drop(opened);
    }
    {
        let connection = rusqlite::Connection::open(path).expect("raw");
        connection
            .execute(
                "UPDATE dbm_sqlite_journal SET state = 'started' WHERE version = ?1",
                [VERSION],
            )
            .expect("mark started");
    }
    let before = fs::read(path).expect("read");
    let error = open_at(path, VERSION, CREATE).expect_err("unknown");
    assert_eq!(error.0, DBM_SQLITE_UNKNOWN_EXECUTION);
    assert!(error.1.contains("unknown execution"));
    assert_eq!(fs::read(path).expect("preserved"), before);
}

#[test]
fn invalid_version_does_not_create_the_file() {
    let scratch = Scratch::new();
    let path = scratch.path.join("missing.sqlite");
    let path = path.to_str().expect("unicode");
    let error = open_at(path, "not-a-version", CREATE).expect_err("version");
    assert_eq!(error.0, DBM_SQLITE_INVALID);
    assert!(!scratch.path.join("missing.sqlite").exists());
}

#[test]
fn doctor_omits_the_path_and_does_not_create_a_file() {
    let scratch = Scratch::new();
    let path = scratch.path.join("secret-path-name.sqlite");
    let path_str = path.to_str().expect("unicode");
    let blob = migration_blob(VERSION, CREATE);
    let path_c = c_string(path_str);
    let app = c_string("1.2.3");
    let mut out: *mut c_char = std::ptr::null_mut();
    let mut err = [0u8; 256];
    let code = unsafe {
        dbm_sqlite_doctor(
            path_c.as_ptr(),
            app.as_ptr(),
            blob.as_ptr(),
            blob.len(),
            &mut out,
            err.as_mut_ptr().cast(),
            err.len(),
        )
    };
    assert_eq!(code, DBM_SQLITE_OK, "{}", read_err(&err));
    let text = unsafe { CStr::from_ptr(out) }
        .to_str()
        .expect("doctor utf-8")
        .to_string();
    unsafe {
        dbm_sqlite_free(out);
    }
    assert!(text.contains("dbm-sqlite 0.1.0"));
    assert!(text.contains("app 1.2.3"));
    assert!(!text.contains("secret-path-name"));
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn open_resolves_a_symlink_directory() {
    let scratch = Scratch::new();
    let real = scratch.path.join("real");
    fs::create_dir(&real).expect("real");
    let link = scratch.path.join("link");
    std::os::unix::fs::symlink(&real, &link).expect("symlink");
    let path = link.join("app.sqlite");
    let path = path.to_str().expect("unicode");
    let opened = open_at(path, VERSION, CREATE).expect("open through link");
    execute(
        opened.handle,
        "INSERT INTO note (body) VALUES ('via-link');",
    )
    .expect("insert");
    drop(opened);
    let reopened = open_at(path, VERSION, CREATE).expect("reopen");
    let json = query_json(reopened.handle, "SELECT body FROM note;").expect("read");
    assert!(json.contains("via-link"));
}
