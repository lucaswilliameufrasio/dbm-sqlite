import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:dbm_sqlite/dbm_sqlite.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:local_ledger/ledger.dart';
import 'package:local_ledger/main.dart';
import 'package:path_provider/path_provider.dart';

const _cipherSelected = bool.fromEnvironment('DBM_SQLITE_SQLCIPHER');

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('selected native variant never silently falls back', (
    tester,
  ) async {
    final root = await getTemporaryDirectory();
    final directory = await root.createTemp('dbm-mobile-cipher-');
    addTearDown(() async {
      await directory.delete(recursive: true);
    });
    final path = '${directory.path}/ledger.sqlite';
    final key = Uint8List.fromList(utf8.encode('test-only-cipher-key'));
    final wrong = Uint8List.fromList(utf8.encode('test-only-wrong-key'));
    addTearDown(() {
      key.fillRange(0, key.length, 0);
      wrong.fillRange(0, wrong.length, 0);
    });
    final migrations = await loadMigrations();
    expect(supportsSqlCipher, _cipherSelected);
    if (!_cipherSelected) {
      expect(
        () => DbmDatabase.openEncrypted(
          path: path,
          migrations: migrations,
          key: key,
        ),
        throwsUnsupportedError,
      );
      expect(File(path).existsSync(), isFalse);
      return;
    }

    final first = DbmDatabase.openEncrypted(
      path: path,
      migrations: [migrations.first],
      key: key,
    );
    first.execute(
      'INSERT INTO expense (payee, amount_cents, spent_on) VALUES (?1, ?2, ?3)',
      ['Padaria', 1250, '2026-11-16'],
    );
    first.close();
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
    expect(wrongReport, isNot(contains('test-only-wrong-key')));
    expect(File(path).readAsBytesSync(), before);

    final database = DbmDatabase.openEncrypted(
      path: path,
      migrations: migrations,
      key: key,
    );
    await tester.pumpWidget(LedgerApp(ledger: Ledger(database)));
    expect(find.textContaining('Padaria'), findsOneWidget);
    await tester.enterText(find.byKey(const Key('payee')), 'Mercado');
    await tester.enterText(find.byKey(const Key('amount')), '20,50');
    await tester.tap(find.byKey(const Key('add')));
    await tester.pumpAndSettle();
    expect(find.textContaining('Mercado'), findsOneWidget);
    await tester.pumpWidget(const SizedBox.shrink());

    final report = doctorEncrypted(
      path: path,
      appVersion: '1.0.0',
      migrations: migrations,
      key: key,
    );
    expect(report, contains('applied'));
    for (final secret in <String>['test-only-cipher-key', 'Padaria', path]) {
      expect(report, isNot(contains(secret)));
    }
    final reopened = Ledger(
      DbmDatabase.openEncrypted(path: path, migrations: migrations, key: key),
    );
    addTearDown(reopened.close);
    expect(reopened.list().length, 2);
    expect(reopened.list().last.category, 'geral');
  });
}
