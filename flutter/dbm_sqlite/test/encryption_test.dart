import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:dbm_sqlite/dbm_sqlite.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final enabled = Platform.environment['DBM_TEST_SQLCIPHER'] == '1';
  test('native SQLCipher availability is explicit', () {
    expect(supportsSqlCipher, enabled);
    if (!enabled) {
      final directory = Directory.systemTemp.createTempSync('dbm-no-cipher-');
      addTearDown(() {
        directory.deleteSync(recursive: true);
      });
      final path = '${directory.path}/unused.sqlite';
      expect(
        () => DbmDatabase.openEncrypted(
          path: path,
          migrations: const <Migration>[],
          key: Uint8List.fromList(<int>[1]),
        ),
        throwsUnsupportedError,
      );
      expect(
        () => doctorEncrypted(
          path: path,
          appVersion: '1.0.0',
          migrations: const <Migration>[],
          key: Uint8List.fromList(<int>[1]),
        ),
        throwsUnsupportedError,
      );
      expect(File(path).existsSync(), isFalse);
    }
  });

  test('ordinary open remains plaintext even when SQLCipher is selected', () {
    final directory = Directory.systemTemp.createTempSync('dbm-plain-control-');
    addTearDown(() {
      directory.deleteSync(recursive: true);
    });
    final path = '${directory.path}/plain.sqlite';
    final database = DbmDatabase.open(
      path: path,
      migrations: const <Migration>[
        Migration(
          version: '20261118000000000001',
          sql: 'CREATE TABLE sample (value TEXT NOT NULL);',
        ),
      ],
    );
    database.close();
    expect(
      File(path).readAsBytesSync().take(16).toList(),
      utf8.encode('SQLite format 3\x00'),
    );
  });

  test('encrypted FFI opens, diagnoses and preserves rows on wrong key', () {
    if (!enabled) {
      markTestSkipped('requires an opt-in SQLCipher native library');
      return;
    }
    final directory = Directory.systemTemp.createTempSync('dbm-encrypted-ffi-');
    addTearDown(() {
      directory.deleteSync(recursive: true);
    });
    final path = '${directory.path}/private-name.sqlite';
    final key = Uint8List.fromList(utf8.encode('synthetic-key'));
    final wrong = Uint8List.fromList(utf8.encode('synthetic-wrong'));
    const migrations = <Migration>[
      Migration(
        version: '20261118000000000001',
        sql: 'CREATE TABLE note (body TEXT NOT NULL);',
      ),
    ];
    final database = DbmDatabase.openEncrypted(
      path: path,
      migrations: migrations,
      key: key,
    );
    database.execute("INSERT INTO note VALUES ('synthetic-row')");
    database.close();
    final before = File(path).readAsBytesSync();
    expect(before.take(16).toList(), isNot(utf8.encode('SQLite format 3\x00')));
    expect(
      () => DbmDatabase.openEncrypted(
        path: path,
        migrations: migrations,
        key: wrong,
      ),
      throwsA(isA<DbmSqliteException>()),
    );
    expect(File(path).readAsBytesSync(), before);
    final wrongReport = doctorEncrypted(
      path: path,
      appVersion: '1.0.0',
      migrations: migrations,
      key: wrong,
    );
    expect(wrongReport, contains('encrypted file unreadable'));
    expect(wrongReport, isNot(contains('synthetic-wrong')));
    expect(File(path).readAsBytesSync(), before);
    final report = doctorEncrypted(
      path: path,
      appVersion: '1.0.0',
      migrations: migrations,
      key: key,
    );
    expect(report, contains('applied'));
    for (final secret in <String>['synthetic-key', 'synthetic-row', path]) {
      expect(report, isNot(contains(secret)));
    }
    final reopened = DbmDatabase.openEncrypted(
      path: path,
      migrations: migrations,
      key: key,
    );
    expect(reopened.query('SELECT body FROM note').rows.single.single,
        'synthetic-row');
    reopened.close();
    key.fillRange(0, key.length, 0);
    wrong.fillRange(0, wrong.length, 0);
  });
}
