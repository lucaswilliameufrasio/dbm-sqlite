#ifndef DBM_SQLITE_H
#define DBM_SQLITE_H

#include <stddef.h>
#include <stdint.h>

/* Codes returned by every call. A non-zero code writes a C string in err. */
#define DBM_SQLITE_OK 0
#define DBM_SQLITE_FAILED 1
#define DBM_SQLITE_OWNED 2
#define DBM_SQLITE_CHECKSUM_MISMATCH 3
#define DBM_SQLITE_UNKNOWN_EXECUTION 4
#define DBM_SQLITE_INVALID 5

/*
 * migrations and args are little-endian blobs owned by the caller.
 * migrations: u32 count, then repeated (u32 len, utf-8 version, u32 len, sql bytes).
 * args: u32 count, then repeated tag. 0 null, 1 i64, 2 f64, 3 utf-8, 4 blob.
 * Text and blob tags are followed by a u32 length and that many bytes.
 */

int32_t dbm_sqlite_open(
    const char *path,
    const uint8_t *migrations,
    size_t migrations_len,
    uint64_t *out_handle,
    char *err,
    size_t err_len);

int32_t dbm_sqlite_execute(
    uint64_t handle,
    const char *sql,
    const uint8_t *args,
    size_t args_len,
    int64_t *out_changes,
    char *err,
    size_t err_len);

int32_t dbm_sqlite_query(
    uint64_t handle,
    const char *sql,
    const uint8_t *args,
    size_t args_len,
    char **out_json,
    char *err,
    size_t err_len);

int32_t dbm_sqlite_doctor(
    const char *path,
    const char *app_version,
    const uint8_t *migrations,
    size_t migrations_len,
    char **out_text,
    char *err,
    size_t err_len);

int32_t dbm_sqlite_close(uint64_t handle, char *err, size_t err_len);

void dbm_sqlite_free(char *ptr);

#endif
