import 'dart:io';

import 'package:dbm_sqlite/dbm_sqlite.dart';
import 'package:dbm_sqlite/src/bindings.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('caller sql is applied and a later edit is refused', () {
    final library = Platform.environment['DBM_SQLITE_LIB'];
    final bundledLinux = Platform.isLinux &&
        Platform.environment['DBM_SQLITE_TEST_BUNDLED_LINUX'] == '1';
    if ((library == null || library.isEmpty) && !bundledLinux) {
      markTestSkipped('DBM_SQLITE_LIB is the host cdylib used by this test');
      return;
    }
    final directory = Directory.systemTemp.createTempSync('dbm-sqlite-dart-');
    addTearDown(() {
      directory.deleteSync(recursive: true);
    });
    final path = '${directory.path}/app.sqlite';
    const version = '20261115000000000001';
    const original = Migration(
      version: version,
      sql: 'CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT);',
    );
    final database = DbmDatabase.open(
      path: path,
      migrations: const <Migration>[original],
    );
    expect(
      database.execute("INSERT INTO note (body) VALUES ('from-dart');"),
      1,
    );
    expect(database.query('SELECT body FROM note;').rows.single.single,
        'from-dart');
    expect(
      () =>
          DbmDatabase.open(path: path, migrations: const <Migration>[original]),
      throwsA(
        isA<DbmSqliteException>().having(
          (error) => error.code,
          'code',
          dbmSqliteOwned,
        ),
      ),
    );
    database.close();

    final reopened = DbmDatabase.open(
      path: path,
      migrations: const <Migration>[original],
    );
    expect(
      reopened.query('SELECT body FROM note;').rows.single.single,
      'from-dart',
    );
    reopened.close();

    expect(
      () => DbmDatabase.open(
        path: path,
        migrations: const <Migration>[
          Migration(
            version: version,
            sql:
                'CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT, extra TEXT);',
          ),
        ],
      ),
      throwsA(
        isA<DbmSqliteException>().having(
          (error) => error.code,
          'code',
          dbmSqliteChecksumMismatch,
        ),
      ),
    );

    final missing = '${directory.path}/secret-path-name.sqlite';
    final report = doctor(
      path: missing,
      appVersion: '1.2.3',
      migrations: const <Migration>[original],
    );
    expect(report, contains('dbm-sqlite 0.1.0'));
    expect(report, isNot(contains('secret-path-name')));
    expect(File(missing).existsSync(), isFalse);
  });
}
