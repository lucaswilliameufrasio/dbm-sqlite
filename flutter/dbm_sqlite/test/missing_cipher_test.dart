import 'dart:io';
import 'dart:typed_data';

import 'package:dbm_sqlite/dbm_sqlite.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('a missing selected SQLCipher binary cannot fall back to SQLite', () {
    if (Platform.environment['DBM_TEST_MISSING_CIPHER'] != '1') {
      markTestSkipped('requires an isolated library directory');
      return;
    }
    final directory =
        Directory.systemTemp.createTempSync('dbm-missing-cipher-');
    addTearDown(() {
      directory.deleteSync(recursive: true);
    });
    final path = '${directory.path}/missing.sqlite';
    expect(
      () => DbmDatabase.openEncrypted(
        path: path,
        migrations: const <Migration>[],
        key: Uint8List.fromList(<int>[1]),
      ),
      throwsA(isA<ArgumentError>()),
    );
    expect(File(path).existsSync(), isFalse);
  });
}
