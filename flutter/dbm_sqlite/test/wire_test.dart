import 'dart:typed_data';

import 'package:dbm_sqlite/dbm_sqlite.dart';
import 'package:dbm_sqlite/src/wire.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('migration bytes match the native fixture', () {
    final bytes = encodeMigrations(const <Migration>[
      Migration(version: '20261115000000000001', sql: 'SELECT 1;'),
    ]);
    expect(
      bytes,
      Uint8List.fromList(const <int>[
        1, 0, 0, 0, //
        20, 0, 0, 0, //
        0x32, 0x30, 0x32, 0x36, 0x31, 0x31, 0x31, 0x35, 0x30, 0x30, //
        0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x31, //
        9, 0, 0, 0, //
        0x53, 0x45, 0x4c, 0x45, 0x43, 0x54, 0x20, 0x31, 0x3b,
      ]),
    );
  });

  test('query json keeps sql types', () {
    final result = decodeQuery(
      '{"columns":["id","body","n","r","b"],"rows":[[{"t":"i","v":7},{"t":"s","v":"a\\"b"},{"t":"n"},{"t":"f","v":1.5},{"t":"b","v":"AQID"}]]}',
    );
    expect(result.columns, <String>['id', 'body', 'n', 'r', 'b']);
    expect(result.rows.single[0], 7);
    expect(result.rows.single[1], 'a"b');
    expect(result.rows.single[2], isNull);
    expect(result.rows.single[3], 1.5);
    final blob = result.rows.single[4];
    expect(blob, isA<Uint8List>());
    expect(blob, Uint8List.fromList(const <int>[1, 2, 3]));
  });
}
