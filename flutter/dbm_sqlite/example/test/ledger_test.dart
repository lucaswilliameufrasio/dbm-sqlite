import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:dbm_sqlite/dbm_sqlite.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:local_ledger/ledger.dart';
import 'package:local_ledger/main.dart';

void main() {
  test('phone sql matches the published checksums', () {
    expect(
      _checksum(File('assets/$versionOne.sql').readAsBytesSync()),
      'sha256:raw:v1:8e10edd69e26b661b2b3ec4863d30ffe75f7500456229b6810bf900767a6fb15',
    );
    expect(
      _checksum(File('assets/$versionTwo.sql').readAsBytesSync()),
      'sha256:raw:v1:e1998649e29823d83771e6c2b1029e9d717276defa4a8cc3e3578f9dbef310b5',
    );
  });

  test('an older file keeps the row when category arrives', () {
    final directory = Directory.systemTemp.createTempSync('local-ledger-');
    addTearDown(() {
      directory.deleteSync(recursive: true);
    });
    final path = '${directory.path}/ledger.sqlite';
    final first = File('assets/$versionOne.sql').readAsStringSync();
    final second = File('assets/$versionTwo.sql').readAsStringSync();
    final opened = DbmDatabase.open(
      path: path,
      migrations: <Migration>[Migration(version: versionOne, sql: first)],
    );
    opened.execute(
      "INSERT INTO expense (payee, amount_cents, spent_on) VALUES ('Padaria', 1250, '2026-11-16')",
    );
    opened.close();

    final ledger = Ledger(
      DbmDatabase.open(
        path: path,
        migrations: <Migration>[
          Migration(version: versionOne, sql: first),
          Migration(version: versionTwo, sql: second),
        ],
      ),
    );
    addTearDown(ledger.close);
    final expense = ledger.list().single;
    expect(expense.payee, 'Padaria');
    expect(expense.amountCents, 1250);
    expect(expense.category, 'geral');
  });

  test('parses reais into cents', () {
    expect(parseAmountReais('12,50'), 1250);
    expect(parseAmountReais('12.5'), 1250);
    expect(parseAmountReais('0,50'), 50);
    expect(() => parseAmountReais('0'), throwsStateError);
    expect(() => parseAmountReais('12.345'), throwsStateError);
  });

  testWidgets('registering an expense shows the row', (tester) async {
    final directory = Directory.systemTemp.createTempSync('local-ledger-ui-');
    addTearDown(() {
      directory.deleteSync(recursive: true);
    });
    final first = File('assets/$versionOne.sql').readAsStringSync();
    final second = File('assets/$versionTwo.sql').readAsStringSync();
    final database = DbmDatabase.open(
      path: '${directory.path}/ledger.sqlite',
      migrations: <Migration>[
        Migration(version: versionOne, sql: first),
        Migration(version: versionTwo, sql: second),
      ],
    );
    await tester.pumpWidget(LedgerApp(ledger: Ledger(database)));
    expect(find.text('Nenhum gasto registrado.'), findsOneWidget);
    await tester.enterText(find.byKey(const Key('payee')), 'Padaria');
    await tester.enterText(find.byKey(const Key('amount')), '12,50');
    await tester.tap(find.byKey(const Key('add')));
    await tester.pump();
    expect(find.byKey(const Key('expense-1')), findsOneWidget);
    expect(find.textContaining('Padaria'), findsOneWidget);
    expect(find.textContaining('R\$ 12,50'), findsOneWidget);
  });
}

String _checksum(List<int> bytes) {
  return 'sha256:raw:v1:${sha256.convert(bytes)}';
}
