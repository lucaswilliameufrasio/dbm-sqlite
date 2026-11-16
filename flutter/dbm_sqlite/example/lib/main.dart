import 'package:dbm_sqlite/dbm_sqlite.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';
import 'package:local_ledger/ledger.dart';
import 'package:path_provider/path_provider.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final directory = await getApplicationDocumentsDirectory();
  final migrations = await loadMigrations();
  final database = DbmDatabase.open(
    path: '${directory.path}/ledger.sqlite',
    migrations: migrations,
  );
  runApp(LedgerApp(ledger: Ledger(database)));
}

Future<List<Migration>> loadMigrations() async {
  final first = await rootBundle.loadString('assets/$versionOne.sql');
  final second = await rootBundle.loadString('assets/$versionTwo.sql');
  return <Migration>[
    Migration(version: versionOne, sql: first),
    Migration(version: versionTwo, sql: second),
  ];
}

class LedgerApp extends StatelessWidget {
  const LedgerApp({required this.ledger, super.key});

  final Ledger ledger;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Caderno de gastos',
      home: LedgerPage(ledger: ledger),
    );
  }
}

class LedgerPage extends StatefulWidget {
  const LedgerPage({required this.ledger, super.key});

  final Ledger ledger;

  @override
  State<LedgerPage> createState() {
    return _LedgerPageState();
  }
}

class _LedgerPageState extends State<LedgerPage> {
  final _payee = TextEditingController();
  final _amount = TextEditingController();
  final _category = TextEditingController(text: 'geral');
  late List<Expense> _expenses;
  String _status = 'Caderno aberto';

  @override
  void initState() {
    super.initState();
    _expenses = widget.ledger.list();
  }

  @override
  void dispose() {
    _payee.dispose();
    _amount.dispose();
    _category.dispose();
    widget.ledger.close();
    super.dispose();
  }

  void _add() {
    final today = DateTime.now();
    final month = today.month.toString().padLeft(2, '0');
    final day = today.day.toString().padLeft(2, '0');
    try {
      widget.ledger.add(
        payee: _payee.text,
        amountReais: _amount.text,
        spentOn: '${today.year}-$month-$day',
        category: _category.text,
      );
      _payee.clear();
      _amount.clear();
      setState(() {
        _expenses = widget.ledger.list();
        _status = 'Gasto registrado';
      });
    } on StateError catch (error) {
      setState(() {
        _status = error.message;
      });
    } on DbmSqliteException catch (error) {
      setState(() {
        _status = error.message;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final rows = <Widget>[
      Text(_status, key: const Key('status')),
    ];
    if (_expenses.isEmpty) {
      rows.add(const Text('Nenhum gasto registrado.', key: Key('expenses')));
    }
    for (final expense in _expenses) {
      rows.add(
        Text(
          '${expense.spentOn}  ${expense.payee}  ${formatCents(expense.amountCents)}  ${expense.category}',
          key: Key('expense-${expense.id}'),
        ),
      );
    }
    return Scaffold(
      appBar: AppBar(title: const Text('Caderno de gastos')),
      body: ListView(
        padding: const EdgeInsets.all(24),
        children: <Widget>[
          const Text('O app aplica o SQL na abertura. O build não chama o Cargo.'),
          const SizedBox(height: 16),
          TextField(
            key: const Key('payee'),
            controller: _payee,
            decoration: const InputDecoration(labelText: 'Quem recebeu'),
          ),
          TextField(
            key: const Key('amount'),
            controller: _amount,
            decoration: const InputDecoration(labelText: 'Valor'),
          ),
          TextField(
            key: const Key('category'),
            controller: _category,
            decoration: const InputDecoration(labelText: 'Categoria'),
          ),
          const SizedBox(height: 12),
          FilledButton(
            key: const Key('add'),
            onPressed: _add,
            child: const Text('Registrar'),
          ),
          const SizedBox(height: 16),
          ...rows,
        ],
      ),
    );
  }
}
