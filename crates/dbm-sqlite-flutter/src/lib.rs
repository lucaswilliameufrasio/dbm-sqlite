//! C ABI used by the Flutter plugin.
//!
//! The functions call [`dbm_sqlite::Database`]. SQL arrives with the call, so
//! editing it does not rebuild this library. A handle refers to the one owner
//! of that file. Another isolate uses the handle instead of opening the file.

mod path;
mod store;
mod wire;

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use dbm_sqlite::ErrorKind;
use rusqlite::params_from_iter;

pub const DBM_SQLITE_OK: i32 = 0;
pub const DBM_SQLITE_FAILED: i32 = 1;
pub const DBM_SQLITE_OWNED: i32 = 2;
pub const DBM_SQLITE_CHECKSUM_MISMATCH: i32 = 3;
pub const DBM_SQLITE_UNKNOWN_EXECUTION: i32 = 4;
pub const DBM_SQLITE_INVALID: i32 = 5;

/// Opens one database and returns the owner handle.
///
/// # Safety
///
/// `path` is a NUL-terminated UTF-8 string. When `migrations_len` is not zero,
/// `migrations` points at that many readable bytes. `out_handle` is writable.
/// `err` is null or writable for `err_len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbm_sqlite_open(
    path: *const c_char,
    migrations: *const u8,
    migrations_len: usize,
    out_handle: *mut u64,
    err: *mut c_char,
    err_len: usize,
) -> i32 {
    guard(err, err_len, || {
        if out_handle.is_null() {
            return Err(invalid("handle output is null"));
        }
        let path = c_str(path)?;
        let bytes = bytes_from(migrations, migrations_len)?;
        let parsed = wire::migrations_from(bytes).map_err(invalid)?;
        let refs = wire::migration_refs(&parsed);
        let resolved = path::resolved_database_path(path).map_err(failed)?;
        let database = dbm_sqlite::Database::open(&resolved, &refs).map_err(open_error)?;
        let id = store::insert(database).map_err(failed)?;
        unsafe {
            *out_handle = id;
        }
        Ok(())
    })
}

/// Opens an encrypted database; available only with the `sqlcipher` feature.
///
/// # Safety
///
/// `path` is NUL-terminated UTF-8. `key` and `migrations` point to readable
/// buffers of the specified lengths. `out_handle` is writable; `err` is null
/// or writable for `err_len` bytes. Key bytes must remain valid for the call.
#[cfg(feature = "sqlcipher")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbm_sqlite_open_encrypted(
    path: *const c_char,
    key: *const u8,
    key_len: usize,
    migrations: *const u8,
    migrations_len: usize,
    out_handle: *mut u64,
    err: *mut c_char,
    err_len: usize,
) -> i32 {
    guard(err, err_len, || {
        if out_handle.is_null() {
            return Err(invalid("handle output is null"));
        }
        unsafe { *out_handle = 0 };
        let key = std::str::from_utf8(bytes_from(key, key_len)?)
            .map_err(|_| invalid("key is not utf-8"))?;
        let path = c_str(path)?;
        let parsed =
            wire::migrations_from(bytes_from(migrations, migrations_len)?).map_err(invalid)?;
        let refs = wire::migration_refs(&parsed);
        let resolved = path::resolved_database_path(path).map_err(|_| failed("invalid path"))?;
        let database =
            dbm_sqlite::Database::open_encrypted(&resolved, &refs, key).map_err(encrypted_error)?;
        let id = store::insert(database).map_err(|_| failed("could not store owner"))?;
        unsafe { *out_handle = id };
        Ok(())
    })
}

/// Runs one statement that does not return rows.
///
/// # Safety
///
/// `sql` is a NUL-terminated UTF-8 string. When `args_len` is not zero, `args`
/// points at that many readable bytes. `out_changes` is writable. `err` is null
/// or writable for `err_len` bytes. `handle` came from [`dbm_sqlite_open`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbm_sqlite_execute(
    handle: u64,
    sql: *const c_char,
    args: *const u8,
    args_len: usize,
    out_changes: *mut i64,
    err: *mut c_char,
    err_len: usize,
) -> i32 {
    guard(err, err_len, || {
        if out_changes.is_null() {
            return Err(invalid("changes output is null"));
        }
        let sql = c_str(sql)?;
        let parsed = wire::args_from(bytes_from(args, args_len)?).map_err(invalid)?;
        let database = store::clone_database(handle).map_err(invalid)?;
        let connection = database.connection().map_err(open_error)?;
        let count = connection
            .execute(sql, params_from_iter(parsed.iter()))
            .map_err(|err| failed(err.to_string()))?;
        let changes =
            i64::try_from(count).map_err(|_| failed("change count does not fit in i64"))?;
        unsafe {
            *out_changes = changes;
        }
        Ok(())
    })
}

/// Runs one statement and returns a JSON payload the caller frees.
///
/// # Safety
///
/// `sql` is a NUL-terminated UTF-8 string. When `args_len` is not zero, `args`
/// points at that many readable bytes. `out_json` is writable. `err` is null or
/// writable for `err_len` bytes. `handle` came from [`dbm_sqlite_open`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbm_sqlite_query(
    handle: u64,
    sql: *const c_char,
    args: *const u8,
    args_len: usize,
    out_json: *mut *mut c_char,
    err: *mut c_char,
    err_len: usize,
) -> i32 {
    guard(err, err_len, || {
        if out_json.is_null() {
            return Err(invalid("json output is null"));
        }
        let sql = c_str(sql)?;
        let parsed = wire::args_from(bytes_from(args, args_len)?).map_err(invalid)?;
        let database = store::clone_database(handle).map_err(invalid)?;
        let connection = database.connection().map_err(open_error)?;
        let mut statement = connection
            .prepare(sql)
            .map_err(|err| failed(err.to_string()))?;
        let columns: Vec<String> = statement.column_names().map(str::to_string).collect();
        let mut rows_iter = statement
            .query(params_from_iter(parsed.iter()))
            .map_err(|err| failed(err.to_string()))?;
        let mut rows = Vec::new();
        loop {
            let next = rows_iter.next().map_err(|err| failed(err.to_string()))?;
            let Some(row) = next else {
                break;
            };
            let mut values = Vec::with_capacity(columns.len());
            for index in 0..columns.len() {
                let value = row.get_ref(index).map_err(|err| failed(err.to_string()))?;
                values.push(wire::ValueRefOwned::from_sql(value).map_err(failed)?);
            }
            rows.push(values);
        }
        let json = wire::encode_row_json(&columns, &rows).map_err(failed)?;
        unsafe {
            *out_json = owned_c_string(json)?;
        }
        Ok(())
    })
}

/// Reads a diagnosis without creating the database file.
///
/// # Safety
///
/// `path` and `app_version` are NUL-terminated UTF-8 strings. When
/// `migrations_len` is not zero, `migrations` points at that many readable
/// bytes. `out_text` is writable. `err` is null or writable for `err_len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbm_sqlite_doctor(
    path: *const c_char,
    app_version: *const c_char,
    migrations: *const u8,
    migrations_len: usize,
    out_text: *mut *mut c_char,
    err: *mut c_char,
    err_len: usize,
) -> i32 {
    guard(err, err_len, || {
        if out_text.is_null() {
            return Err(invalid("doctor output is null"));
        }
        let path = c_str(path)?;
        let app_version = c_str(app_version)?;
        let parsed =
            wire::migrations_from(bytes_from(migrations, migrations_len)?).map_err(invalid)?;
        let refs = wire::migration_refs(&parsed);
        let report_path = match path::resolved_database_path(path) {
            Ok(resolved) => resolved,
            Err(_) => std::path::PathBuf::from(path),
        };
        let text = dbm_sqlite::doctor(&report_path, app_version, &refs).to_string();
        unsafe {
            *out_text = owned_c_string(text)?;
        }
        Ok(())
    })
}

/// Reads an encrypted diagnosis; available only with `sqlcipher`.
///
/// # Safety
///
/// `path` and `app_version` are NUL-terminated UTF-8 strings. `key` and
/// `migrations` point to readable buffers of the specified lengths. `out_text`
/// is writable; `err` is null or writable for `err_len` bytes.
#[cfg(feature = "sqlcipher")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbm_sqlite_doctor_encrypted(
    path: *const c_char,
    app_version: *const c_char,
    key: *const u8,
    key_len: usize,
    migrations: *const u8,
    migrations_len: usize,
    out_text: *mut *mut c_char,
    err: *mut c_char,
    err_len: usize,
) -> i32 {
    guard(err, err_len, || {
        if out_text.is_null() {
            return Err(invalid("doctor output is null"));
        }
        unsafe { *out_text = ptr::null_mut() };
        let key = std::str::from_utf8(bytes_from(key, key_len)?)
            .map_err(|_| invalid("key is not utf-8"))?;
        let path = c_str(path)?;
        let app_version = c_str(app_version)?;
        let parsed =
            wire::migrations_from(bytes_from(migrations, migrations_len)?).map_err(invalid)?;
        let refs = wire::migration_refs(&parsed);
        let report_path = match path::resolved_database_path(path) {
            Ok(resolved) => resolved,
            Err(_) => std::path::PathBuf::from(path),
        };
        let report = dbm_sqlite::doctor_encrypted(&report_path, app_version, &refs, key)
            .map_err(encrypted_error)?;
        unsafe { *out_text = owned_c_string(report.to_string())? };
        Ok(())
    })
}

/// Drops the owner for `handle`.
///
/// # Safety
///
/// `err` is null or writable for `err_len` bytes. `handle` came from
/// [`dbm_sqlite_open`] and has not been closed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbm_sqlite_close(handle: u64, err: *mut c_char, err_len: usize) -> i32 {
    guard(err, err_len, || {
        store::remove(handle).map_err(invalid)?;
        Ok(())
    })
}

/// Frees a string returned by this library.
///
/// # Safety
///
/// `ptr` is null or a pointer returned by [`dbm_sqlite_query`] or
/// [`dbm_sqlite_doctor`] that has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbm_sqlite_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        drop(unsafe { CString::from_raw(ptr) });
    }));
}

struct Status {
    code: i32,
    message: String,
}

fn invalid(message: impl Into<String>) -> Status {
    Status {
        code: DBM_SQLITE_INVALID,
        message: message.into(),
    }
}

fn failed(message: impl Into<String>) -> Status {
    Status {
        code: DBM_SQLITE_FAILED,
        message: message.into(),
    }
}

fn open_error(error: dbm_sqlite::Error) -> Status {
    let code = match error.kind() {
        ErrorKind::Owned => DBM_SQLITE_OWNED,
        ErrorKind::ChecksumMismatch { .. } => DBM_SQLITE_CHECKSUM_MISMATCH,
        ErrorKind::UnknownExecution { .. } => DBM_SQLITE_UNKNOWN_EXECUTION,
        ErrorKind::InvalidVersion { .. }
        | ErrorKind::DuplicateVersion { .. }
        | ErrorKind::EmptySql { .. }
        | ErrorKind::InvalidSql { .. }
        | ErrorKind::UnsafeSql { .. } => DBM_SQLITE_INVALID,
        _ => DBM_SQLITE_FAILED,
    };
    Status {
        code,
        message: error.to_string(),
    }
}

#[cfg(feature = "sqlcipher")]
fn encrypted_error(error: dbm_sqlite::Error) -> Status {
    let code = match error.kind() {
        ErrorKind::Owned => DBM_SQLITE_OWNED,
        ErrorKind::ChecksumMismatch { .. } => DBM_SQLITE_CHECKSUM_MISMATCH,
        ErrorKind::UnknownExecution { .. } => DBM_SQLITE_UNKNOWN_EXECUTION,
        ErrorKind::InvalidEncryptionKey
        | ErrorKind::InvalidVersion { .. }
        | ErrorKind::DuplicateVersion { .. }
        | ErrorKind::EmptySql { .. }
        | ErrorKind::InvalidSql { .. }
        | ErrorKind::UnsafeSql { .. } => DBM_SQLITE_INVALID,
        _ => DBM_SQLITE_FAILED,
    };
    Status {
        code,
        message: "encrypted database operation failed".to_string(),
    }
}

fn guard(err: *mut c_char, err_len: usize, body: impl FnOnce() -> Result<(), Status>) -> i32 {
    clear_err(err, err_len);
    let result = catch_unwind(AssertUnwindSafe(body));
    match result {
        Ok(Ok(())) => DBM_SQLITE_OK,
        Ok(Err(status)) => {
            write_err(err, err_len, &status.message);
            status.code
        }
        Err(_) => {
            write_err(err, err_len, "native call panicked");
            DBM_SQLITE_FAILED
        }
    }
}

fn c_str<'a>(ptr: *const c_char) -> Result<&'a str, Status> {
    if ptr.is_null() {
        return Err(invalid("string is null"));
    }
    let text = unsafe { CStr::from_ptr(ptr) };
    text.to_str().map_err(|_| invalid("string is not utf-8"))
}

fn bytes_from<'a>(ptr: *const u8, len: usize) -> Result<&'a [u8], Status> {
    if len == 0 {
        return Ok(&[]);
    }
    if ptr.is_null() {
        return Err(invalid("blob is null"));
    }
    Ok(unsafe { std::slice::from_raw_parts(ptr, len) })
}

fn owned_c_string(text: String) -> Result<*mut c_char, Status> {
    match CString::new(text) {
        Ok(value) => Ok(value.into_raw()),
        Err(_) => Err(failed("text contains a nul byte")),
    }
}

fn clear_err(err: *mut c_char, err_len: usize) {
    if err.is_null() || err_len == 0 {
        return;
    }
    unsafe {
        *err = 0;
    }
}

fn write_err(err: *mut c_char, err_len: usize, message: &str) {
    if err.is_null() || err_len == 0 {
        return;
    }
    let max = err_len - 1;
    let mut end = message.len().min(max);
    while end > 0 && !message.is_char_boundary(end) {
        end -= 1;
    }
    unsafe {
        ptr::copy_nonoverlapping(message.as_ptr(), err.cast(), end);
        *err.add(end) = 0;
    }
}
