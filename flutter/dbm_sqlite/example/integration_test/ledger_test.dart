import 'package:dbm_sqlite/dbm_sqlite.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:local_ledger/ledger.dart';
import 'package:local_ledger/main.dart';
import 'temp_directory.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('upgrades an existing file and persists a new expense', (
    tester,
  ) async {
    final directory = await integrationTemp('dbm-ledger-integration-');
    addTearDown(() async {
      await directory.delete(recursive: true);
    });
    final path = '${directory.path}/ledger.sqlite';
    final migrations = await loadMigrations();
    final first = DbmDatabase.open(path: path, migrations: [migrations.first]);
    first.execute(
      'INSERT INTO expense (payee, amount_cents, spent_on) VALUES (?1, ?2, ?3)',
      ['Padaria', 1250, '2026-11-16'],
    );
    first.close();

    final upgraded = DbmDatabase.open(path: path, migrations: migrations);
    final ledger = Ledger(upgraded);
    expect(ledger.list().single.category, 'geral');
    await tester.pumpWidget(LedgerApp(ledger: ledger));
    expect(find.textContaining('Padaria'), findsOneWidget);
    await tester.enterText(find.byKey(const Key('payee')), 'Mercado');
    await tester.enterText(find.byKey(const Key('amount')), '20,50');
    await tester.tap(find.byKey(const Key('add')));
    await tester.pumpAndSettle();
    expect(find.textContaining('Mercado'), findsOneWidget);
    await tester.pumpWidget(const SizedBox.shrink());

    final reopened = Ledger(
      DbmDatabase.open(path: path, migrations: migrations),
    );
    addTearDown(reopened.close);
    final expenses = reopened.list();
    expect(expenses.length, 2);
    expect(expenses.first.payee, 'Mercado');
    expect(expenses.first.amountCents, 2050);
    expect(expenses.last.payee, 'Padaria');
    expect(expenses.last.category, 'geral');
  });

  testWidgets('rejects changed migration without losing existing rows', (
    tester,
  ) async {
    final directory = await integrationTemp('dbm-ledger-checksum-');
    addTearDown(() async {
      await directory.delete(recursive: true);
    });
    final path = '${directory.path}/ledger.sqlite';
    final migrations = await loadMigrations();
    final database = DbmDatabase.open(path: path, migrations: migrations);
    database.execute(
      'INSERT INTO expense (payee, amount_cents, spent_on, category) '
      'VALUES (?1, ?2, ?3, ?4)',
      ['Padaria', 1250, '2026-11-16', 'geral'],
    );
    database.close();
    expect(
      () => DbmDatabase.open(
        path: path,
        migrations: [
          Migration(
            version: migrations.first.version,
            sql: '${migrations.first.sql}\n-- changed',
          ),
          migrations.last,
        ],
      ),
      throwsA(isA<DbmSqliteException>()),
    );
    final reopened = Ledger(
      DbmDatabase.open(path: path, migrations: migrations),
    );
    addTearDown(reopened.close);
    expect(reopened.list().single.payee, 'Padaria');
  });
}
