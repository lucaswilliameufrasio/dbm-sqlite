import 'dart:convert';
import 'dart:typed_data';

import 'package:dbm_sqlite/src/types.dart';

/// Little-endian blob the native library reads.
///
/// `u32 count`, then for each migration `u32` byte length, UTF-8 version,
/// `u32` byte length, and the SQL bytes. The Dart encoder and the Rust parser
/// both accept the fixture in `test/wire_test.dart`.
Uint8List encodeMigrations(List<Migration> migrations) {
  final builder = BytesBuilder(copy: false);
  _addU32(builder, migrations.length);
  for (final migration in migrations) {
    _addBytes(builder, utf8.encode(migration.version));
    _addBytes(builder, utf8.encode(migration.sql));
  }
  return builder.toBytes();
}

/// `u32 count`, then a tag byte. 0 null, 1 i64, 2 f64, 3 text, 4 blob.
Uint8List encodeArgs(List<Object?> args) {
  final builder = BytesBuilder(copy: false);
  _addU32(builder, args.length);
  for (final arg in args) {
    _addArg(builder, arg);
  }
  return builder.toBytes();
}

QueryResult decodeQuery(String json) {
  final decoded = jsonDecode(json);
  if (decoded is! Map<String, Object?>) {
    throw const FormatException('query result is not an object');
  }
  final rawColumns = decoded['columns'];
  final rawRows = decoded['rows'];
  if (rawColumns is! List<Object?> || rawRows is! List<Object?>) {
    throw const FormatException('query result is missing columns or rows');
  }
  final columns = <String>[];
  for (final column in rawColumns) {
    if (column is! String) {
      throw const FormatException('query column is not text');
    }
    columns.add(column);
  }
  final rows = <List<Object?>>[];
  for (final rawRow in rawRows) {
    if (rawRow is! List<Object?>) {
      throw const FormatException('query row is not a list');
    }
    if (rawRow.length != columns.length) {
      throw const FormatException('query row width does not match columns');
    }
    final row = <Object?>[];
    for (final cell in rawRow) {
      row.add(_decodeCell(cell));
    }
    rows.add(row);
  }
  return QueryResult(columns, rows);
}

Object? _decodeCell(Object? cell) {
  if (cell is! Map<String, Object?>) {
    throw const FormatException('query cell is not an object');
  }
  final tag = cell['t'];
  if (tag == 'n') {
    return null;
  }
  if (tag == 'i') {
    final value = cell['v'];
    if (value is! int) {
      throw const FormatException('query integer is not an int');
    }
    return value;
  }
  if (tag == 'f') {
    final value = cell['v'];
    if (value is! num) {
      throw const FormatException('query real is not a number');
    }
    return value.toDouble();
  }
  if (tag == 's') {
    final value = cell['v'];
    if (value is! String) {
      throw const FormatException('query text is not a string');
    }
    return value;
  }
  if (tag == 'b') {
    final value = cell['v'];
    if (value is! String) {
      throw const FormatException('query blob is not base64');
    }
    return base64Decode(value);
  }
  throw FormatException('unknown query cell tag $tag');
}

void _addArg(BytesBuilder builder, Object? arg) {
  if (arg == null) {
    builder.add(const <int>[0]);
    return;
  }
  if (arg is int) {
    builder.add(const <int>[1]);
    final data = ByteData(8)..setInt64(0, arg, Endian.little);
    builder.add(data.buffer.asUint8List());
    return;
  }
  if (arg is double) {
    builder.add(const <int>[2]);
    final data = ByteData(8)..setFloat64(0, arg, Endian.little);
    builder.add(data.buffer.asUint8List());
    return;
  }
  if (arg is String) {
    builder.add(const <int>[3]);
    _addBytes(builder, utf8.encode(arg));
    return;
  }
  if (arg is Uint8List) {
    builder.add(const <int>[4]);
    _addBytes(builder, arg);
    return;
  }
  throw ArgumentError('unsupported SQL argument ${arg.runtimeType}');
}

void _addBytes(BytesBuilder builder, List<int> bytes) {
  _addU32(builder, bytes.length);
  builder.add(bytes);
}

void _addU32(BytesBuilder builder, int value) {
  final data = ByteData(4)..setUint32(0, value, Endian.little);
  builder.add(data.buffer.asUint8List());
}
