import 'dart:convert';
import 'dart:ffi';
import 'dart:io';

import 'package:dbm_sqlite/src/types.dart';
import 'package:ffi/ffi.dart';

const dbmSqliteOk = 0;
const dbmSqliteFailed = 1;
const dbmSqliteOwned = 2;
const dbmSqliteChecksumMismatch = 3;
const dbmSqliteUnknownExecution = 4;
const dbmSqliteInvalid = 5;

const errorBytes = 4096;

final class DbmBindings {
  DbmBindings(DynamicLibrary library)
      : open = library.lookupFunction<_OpenNative, _Open>('dbm_sqlite_open'),
        execute = library.lookupFunction<_ExecuteNative, _Execute>(
          'dbm_sqlite_execute',
        ),
        query =
            library.lookupFunction<_QueryNative, _Query>('dbm_sqlite_query'),
        doctor = library.lookupFunction<_DoctorNative, _Doctor>(
          'dbm_sqlite_doctor',
        ),
        close =
            library.lookupFunction<_CloseNative, _Close>('dbm_sqlite_close'),
        free = library.lookupFunction<_FreeNative, _Free>('dbm_sqlite_free');

  final _Open open;
  final _Execute execute;
  final _Query query;
  final _Doctor doctor;
  final _Close close;
  final _Free free;
}

DbmBindings? _cached;

DbmBindings loadBindings() {
  final existing = _cached;
  if (existing != null) {
    return existing;
  }
  final loaded = DbmBindings(_openLibrary());
  _cached = loaded;
  return loaded;
}

DynamicLibrary _openLibrary() {
  if (Platform.isAndroid) {
    return DynamicLibrary.open('libdbm_sqlite.so');
  }
  if (Platform.isIOS) {
    return DynamicLibrary.open('DbmSqlite.framework/DbmSqlite');
  }
  final override = Platform.environment['DBM_SQLITE_LIB'];
  if (override != null && override.isNotEmpty) {
    return DynamicLibrary.open(override);
  }
  throw UnsupportedError(
    'dbm_sqlite ships binaries for Android and iOS.',
  );
}

DbmSqliteException nativeError(int code, Pointer<Uint8> err) {
  return DbmSqliteException(code, readNativeString(err, errorBytes));
}

String readNativeString(Pointer<Uint8> bytes, int capacity) {
  var length = capacity;
  for (var index = 0; index < capacity; index = index + 1) {
    if (bytes[index] == 0) {
      length = index;
      break;
    }
  }
  final copy = <int>[];
  for (var index = 0; index < length; index = index + 1) {
    copy.add(bytes[index]);
  }
  return utf8.decode(copy);
}

typedef _OpenNative = Int32 Function(
  Pointer<Utf8> path,
  Pointer<Uint8> migrations,
  IntPtr migrationsLen,
  Pointer<Uint64> outHandle,
  Pointer<Uint8> err,
  IntPtr errLen,
);
typedef _Open = int Function(
  Pointer<Utf8> path,
  Pointer<Uint8> migrations,
  int migrationsLen,
  Pointer<Uint64> outHandle,
  Pointer<Uint8> err,
  int errLen,
);

typedef _ExecuteNative = Int32 Function(
  Uint64 handle,
  Pointer<Utf8> sql,
  Pointer<Uint8> args,
  IntPtr argsLen,
  Pointer<Int64> outChanges,
  Pointer<Uint8> err,
  IntPtr errLen,
);
typedef _Execute = int Function(
  int handle,
  Pointer<Utf8> sql,
  Pointer<Uint8> args,
  int argsLen,
  Pointer<Int64> outChanges,
  Pointer<Uint8> err,
  int errLen,
);

typedef _QueryNative = Int32 Function(
  Uint64 handle,
  Pointer<Utf8> sql,
  Pointer<Uint8> args,
  IntPtr argsLen,
  Pointer<Pointer<Utf8>> outJson,
  Pointer<Uint8> err,
  IntPtr errLen,
);
typedef _Query = int Function(
  int handle,
  Pointer<Utf8> sql,
  Pointer<Uint8> args,
  int argsLen,
  Pointer<Pointer<Utf8>> outJson,
  Pointer<Uint8> err,
  int errLen,
);

typedef _DoctorNative = Int32 Function(
  Pointer<Utf8> path,
  Pointer<Utf8> appVersion,
  Pointer<Uint8> migrations,
  IntPtr migrationsLen,
  Pointer<Pointer<Utf8>> outText,
  Pointer<Uint8> err,
  IntPtr errLen,
);
typedef _Doctor = int Function(
  Pointer<Utf8> path,
  Pointer<Utf8> appVersion,
  Pointer<Uint8> migrations,
  int migrationsLen,
  Pointer<Pointer<Utf8>> outText,
  Pointer<Uint8> err,
  int errLen,
);

typedef _CloseNative = Int32 Function(
  Uint64 handle,
  Pointer<Uint8> err,
  IntPtr errLen,
);
typedef _Close = int Function(int handle, Pointer<Uint8> err, int errLen);

typedef _FreeNative = Void Function(Pointer<Utf8> ptr);
typedef _Free = void Function(Pointer<Utf8> ptr);
