import 'package:dbm_sqlite/dbm_sqlite.dart';

const versionOne = '20261116000000000001';
const versionTwo = '20261116000000000002';

final class Expense {
  const Expense({
    required this.id,
    required this.payee,
    required this.amountCents,
    required this.spentOn,
    required this.category,
  });

  final int id;
  final String payee;
  final int amountCents;
  final String spentOn;
  final String category;
}

final class Ledger {
  Ledger(this._database);

  final DbmDatabase _database;

  void add({
    required String payee,
    required String amountReais,
    required String spentOn,
    required String category,
  }) {
    final trimmedPayee = payee.trim();
    final trimmedCategory = category.trim();
    if (trimmedPayee.isEmpty) {
      throw StateError('O favorecido não pode ser vazio');
    }
    if (trimmedCategory.isEmpty) {
      throw StateError('A categoria não pode ser vazia');
    }
    if (!_validDate(spentOn)) {
      throw StateError('O dia tem de estar em AAAA-MM-DD');
    }
    final cents = parseAmountReais(amountReais);
    _database.execute(
      'INSERT INTO expense (payee, amount_cents, spent_on, category) '
      'VALUES (?1, ?2, ?3, ?4)',
      <Object?>[trimmedPayee, cents, spentOn, trimmedCategory],
    );
  }

  List<Expense> list() {
    final result = _database.query(
      'SELECT id, payee, amount_cents, spent_on, category '
      'FROM expense ORDER BY id DESC',
    );
    final expenses = <Expense>[];
    for (final row in result.rows) {
      expenses.add(
        Expense(
          id: row[0] as int,
          payee: row[1] as String,
          amountCents: row[2] as int,
          spentOn: row[3] as String,
          category: row[4] as String,
        ),
      );
    }
    return expenses;
  }

  void close() {
    _database.close();
  }
}

int parseAmountReais(String text) {
  final trimmed = text.trim();
  if (trimmed.isEmpty || (trimmed.contains(',') && trimmed.contains('.'))) {
    throw StateError('O valor tem de ser maior que zero');
  }
  final normalized = trimmed.contains(',')
      ? trimmed.replaceAll(',', '.')
      : trimmed;
  final parts = normalized.split('.');
  if (parts.length > 2) {
    throw StateError('O valor tem de ser maior que zero');
  }
  final whole = parts[0];
  if (whole.isEmpty ||
      !_digits(whole) ||
      (whole.length > 1 && whole.startsWith('0'))) {
    throw StateError('O valor tem de ser maior que zero');
  }
  var fraction = '00';
  if (parts.length == 2) {
    final value = parts[1];
    if (value.isEmpty || value.length > 2 || !_digits(value)) {
      throw StateError('O valor tem de ser maior que zero');
    }
    fraction = value.length == 1 ? '${value}0' : value;
  }
  final cents = int.parse('$whole$fraction');
  if (cents <= 0) {
    throw StateError('O valor tem de ser maior que zero');
  }
  return cents;
}

String formatCents(int cents) {
  final whole = cents ~/ 100;
  final fraction = cents.abs() % 100;
  return 'R\$ $whole,${fraction.toString().padLeft(2, '0')}';
}

bool _digits(String value) {
  for (final code in value.codeUnits) {
    if (code < 0x30 || code > 0x39) {
      return false;
    }
  }
  return true;
}

bool _validDate(String value) {
  if (value.length != 10) {
    return false;
  }
  for (var index = 0; index < value.length; index = index + 1) {
    final code = value.codeUnitAt(index);
    if (index == 4 || index == 7) {
      if (code != 0x2d) {
        return false;
      }
    } else if (code < 0x30 || code > 0x39) {
      return false;
    }
  }
  return true;
}
