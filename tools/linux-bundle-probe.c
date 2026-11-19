#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "dbm_sqlite.h"

typedef int32_t (*open_fn)(const char *, const uint8_t *, size_t, uint64_t *,
                           char *, size_t);
typedef int32_t (*execute_fn)(uint64_t, const char *, const uint8_t *, size_t,
                              int64_t *, char *, size_t);
typedef int32_t (*query_fn)(uint64_t, const char *, const uint8_t *, size_t,
                            char **, char *, size_t);
typedef int32_t (*close_fn)(uint64_t, char *, size_t);
typedef void (*free_fn)(char *);

int run_bundle_probe(const char *path) {
  void *library = dlopen("libdbm_sqlite_flutter.so", RTLD_NOW | RTLD_LOCAL);
  if (library == NULL) {
    fprintf(stderr, "loading bundled library failed: %s\n", dlerror());
    return 1;
  }
  open_fn open_database = (open_fn)dlsym(library, "dbm_sqlite_open");
  execute_fn execute = (execute_fn)dlsym(library, "dbm_sqlite_execute");
  query_fn query = (query_fn)dlsym(library, "dbm_sqlite_query");
  close_fn close_database = (close_fn)dlsym(library, "dbm_sqlite_close");
  free_fn free_result = (free_fn)dlsym(library, "dbm_sqlite_free");
  if (open_database == NULL || execute == NULL || query == NULL ||
      close_database == NULL || free_result == NULL) {
    fprintf(stderr, "missing ABI entry point\n");
    dlclose(library);
    return 1;
  }

  const uint8_t empty[] = {0, 0, 0, 0};
  char error[4096] = {0};
  uint64_t handle = 0;
  int64_t changes = 0;
  char *result = NULL;
  if (open_database(path, empty, sizeof(empty), &handle, error, sizeof(error)) !=
          DBM_SQLITE_OK ||
      execute(handle, "CREATE TABLE note (body TEXT)", empty, sizeof(empty),
              &changes, error, sizeof(error)) != DBM_SQLITE_OK ||
      execute(handle, "INSERT INTO note VALUES ('bundle-sentinel')", empty,
              sizeof(empty), &changes, error, sizeof(error)) != DBM_SQLITE_OK ||
      close_database(handle, error, sizeof(error)) != DBM_SQLITE_OK) {
    fprintf(stderr, "bundled database setup failed: %s\n", error);
    dlclose(library);
    return 1;
  }
  if (open_database(path, empty, sizeof(empty), &handle, error, sizeof(error)) !=
          DBM_SQLITE_OK ||
      query(handle, "SELECT body FROM note", empty, sizeof(empty), &result,
            error, sizeof(error)) != DBM_SQLITE_OK) {
    fprintf(stderr, "bundled database reopen failed: %s\n", error);
    dlclose(library);
    return 1;
  }
  int found = result != NULL && strstr(result, "bundle-sentinel") != NULL;
  free_result(result);
  if (close_database(handle, error, sizeof(error)) != DBM_SQLITE_OK || !found) {
    fprintf(stderr, "bundled database result was not preserved\n");
    dlclose(library);
    return 1;
  }
  dlclose(library);
  puts("bundled ABI: created, wrote, reopened and read SQLite");
  return 0;
}
