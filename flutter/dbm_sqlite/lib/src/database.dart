import 'dart:ffi';
import 'dart:typed_data';

import 'package:dbm_sqlite/src/bindings.dart';
import 'package:dbm_sqlite/src/types.dart';
import 'package:dbm_sqlite/src/wire.dart';
import 'package:ffi/ffi.dart';

export 'package:dbm_sqlite/src/types.dart';

/// The one owner of a SQLite file.
///
/// [open] migrates, then this value holds the only writable connection.
/// A second [open] of that file fails and does not write. Another isolate
/// uses [fromHandle] with the integer from [handle]. It must not open the
/// file itself. Call [close] once.
final class DbmDatabase {
  DbmDatabase._(this._handle);

  /// Wraps a handle another isolate already opened.
  DbmDatabase.fromHandle(int handle) : _handle = handle {
    if (handle == 0) {
      throw ArgumentError('database handle is empty');
    }
  }

  int _handle;

  /// Native owner id. Send this to a background isolate.
  int get handle {
    return _handle;
  }

  /// Opens [path] and applies pending [migrations].
  ///
  /// The parent directory must already exist. [migrations] may be empty.
  /// This call blocks the isolate that runs it.
  static DbmDatabase open({
    required String path,
    required List<Migration> migrations,
  }) {
    final native = loadBindings();
    final pathPtr = path.toNativeUtf8();
    final blob = encodeMigrations(migrations);
    final outHandle = calloc<Uint64>();
    final err = calloc<Uint8>(errorBytes);
    try {
      final code = _withBytes(blob, (pointer, length) {
        return native.open(
          pathPtr,
          pointer,
          length,
          outHandle,
          err,
          errorBytes,
        );
      });
      if (code != dbmSqliteOk) {
        throw nativeError(code, err);
      }
      return DbmDatabase._(outHandle.value);
    } finally {
      calloc.free(outHandle);
      calloc.free(err);
      malloc.free(pathPtr);
    }
  }

  /// Opens an encrypted database using a UTF-8 passphrase in [key].
  ///
  /// Requires a native library built with SQLCipher. The caller owns [key]
  /// and should clear it when no longer needed. Dart and SQLCipher may retain
  /// additional copies; this method cannot guarantee complete zeroization.
  static DbmDatabase openEncrypted({
    required String path,
    required List<Migration> migrations,
    required Uint8List key,
  }) {
    final native = loadBindings();
    final open = native.openEncrypted;
    final pathPtr = path.toNativeUtf8();
    final blob = encodeMigrations(migrations);
    final outHandle = calloc<Uint64>();
    final err = calloc<Uint8>(errorBytes);
    try {
      final code = _withSecret(key, (keyPtr, keyLength) {
        return _withBytes(blob, (pointer, length) {
          return open(
            pathPtr,
            keyPtr,
            keyLength,
            pointer,
            length,
            outHandle,
            err,
            errorBytes,
          );
        });
      });
      if (code != dbmSqliteOk) {
        throw nativeError(code, err);
      }
      return DbmDatabase._(outHandle.value);
    } finally {
      calloc.free(outHandle);
      calloc.free(err);
      malloc.free(pathPtr);
    }
  }

  /// Runs one statement that does not return rows.
  int execute(String sql, [List<Object?> args = const <Object?>[]]) {
    final native = loadBindings();
    final sqlPtr = sql.toNativeUtf8();
    final blob = encodeArgs(args);
    final outChanges = calloc<Int64>();
    final err = calloc<Uint8>(errorBytes);
    try {
      final code = _withBytes(blob, (pointer, length) {
        return native.execute(
          _requireHandle(),
          sqlPtr,
          pointer,
          length,
          outChanges,
          err,
          errorBytes,
        );
      });
      if (code != dbmSqliteOk) {
        throw nativeError(code, err);
      }
      return outChanges.value;
    } finally {
      calloc.free(outChanges);
      calloc.free(err);
      malloc.free(sqlPtr);
    }
  }

  /// Runs one statement and returns every row.
  QueryResult query(String sql, [List<Object?> args = const <Object?>[]]) {
    final native = loadBindings();
    final sqlPtr = sql.toNativeUtf8();
    final blob = encodeArgs(args);
    final outJson = calloc<Pointer<Utf8>>();
    final err = calloc<Uint8>(errorBytes);
    try {
      final code = _withBytes(blob, (pointer, length) {
        return native.query(
          _requireHandle(),
          sqlPtr,
          pointer,
          length,
          outJson,
          err,
          errorBytes,
        );
      });
      if (code != dbmSqliteOk) {
        throw nativeError(code, err);
      }
      final pointer = outJson.value;
      if (pointer == nullptr) {
        throw StateError('query returned no payload');
      }
      final json = pointer.toDartString();
      native.free(pointer);
      return decodeQuery(json);
    } finally {
      calloc.free(outJson);
      calloc.free(err);
      malloc.free(sqlPtr);
    }
  }

  /// Drops the native owner. A later call throws.
  void close() {
    if (_handle == 0) {
      return;
    }
    final native = loadBindings();
    final err = calloc<Uint8>(errorBytes);
    final handle = _handle;
    _handle = 0;
    try {
      final code = native.close(handle, err, errorBytes);
      if (code != dbmSqliteOk) {
        throw nativeError(code, err);
      }
    } finally {
      calloc.free(err);
    }
  }

  int _requireHandle() {
    if (_handle == 0) {
      throw StateError('database is closed');
    }
    return _handle;
  }
}

/// Whether the loaded native binary exposes the optional SQLCipher ABI.
bool get supportsSqlCipher {
  return loadBindings().supportsEncryption;
}

/// Reads an encrypted journal without applying migrations.
String doctorEncrypted({
  required String path,
  required String appVersion,
  required List<Migration> migrations,
  required Uint8List key,
}) {
  final native = loadBindings();
  final encryptedDoctor = native.doctorEncrypted;
  final pathPtr = path.toNativeUtf8();
  final appPtr = appVersion.toNativeUtf8();
  final blob = encodeMigrations(migrations);
  final outText = calloc<Pointer<Utf8>>();
  final err = calloc<Uint8>(errorBytes);
  try {
    final code = _withSecret(key, (keyPtr, keyLength) {
      return _withBytes(blob, (pointer, length) {
        return encryptedDoctor(
          pathPtr,
          appPtr,
          keyPtr,
          keyLength,
          pointer,
          length,
          outText,
          err,
          errorBytes,
        );
      });
    });
    if (code != dbmSqliteOk) {
      throw nativeError(code, err);
    }
    final pointer = outText.value;
    if (pointer == nullptr) {
      throw StateError('doctor returned no text');
    }
    try {
      return pointer.toDartString();
    } finally {
      native.free(pointer);
    }
  } finally {
    calloc.free(outText);
    calloc.free(err);
    malloc.free(pathPtr);
    malloc.free(appPtr);
  }
}

/// Reads the journal without opening a writable connection or creating a file.
String doctor({
  required String path,
  required String appVersion,
  required List<Migration> migrations,
}) {
  final native = loadBindings();
  final pathPtr = path.toNativeUtf8();
  final appPtr = appVersion.toNativeUtf8();
  final blob = encodeMigrations(migrations);
  final outText = calloc<Pointer<Utf8>>();
  final err = calloc<Uint8>(errorBytes);
  try {
    final code = _withBytes(blob, (pointer, length) {
      return native.doctor(
        pathPtr,
        appPtr,
        pointer,
        length,
        outText,
        err,
        errorBytes,
      );
    });
    if (code != dbmSqliteOk) {
      throw nativeError(code, err);
    }
    final pointer = outText.value;
    if (pointer == nullptr) {
      throw StateError('doctor returned no text');
    }
    final text = pointer.toDartString();
    native.free(pointer);
    return text;
  } finally {
    calloc.free(outText);
    calloc.free(err);
    malloc.free(pathPtr);
    malloc.free(appPtr);
  }
}

T _withBytes<T>(Uint8List bytes, T Function(Pointer<Uint8>, int) body) {
  if (bytes.isEmpty) {
    return body(nullptr, 0);
  }
  final native = malloc<Uint8>(bytes.length);
  try {
    native.asTypedList(bytes.length).setAll(0, bytes);
    return body(native, bytes.length);
  } finally {
    malloc.free(native);
  }
}

T _withSecret<T>(Uint8List bytes, T Function(Pointer<Uint8>, int) body) {
  if (bytes.isEmpty) {
    return body(nullptr, 0);
  }
  final native = malloc<Uint8>(bytes.length);
  final copy = native.asTypedList(bytes.length);
  try {
    copy.setAll(0, bytes);
    return body(native, bytes.length);
  } finally {
    copy.fillRange(0, copy.length, 0);
    malloc.free(native);
  }
}
