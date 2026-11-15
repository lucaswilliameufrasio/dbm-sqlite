/// One timestamped SQL migration.
///
/// [version] is a 20-digit UTC timestamp, `YYYYMMDDHHMMSSffffff`. [sql] is
/// hashed as its exact bytes. The native library does not contain this text.
final class Migration {
  const Migration({required this.version, required this.sql});

  final String version;
  final String sql;
}

/// Rows from one query. [rows] uses null, int, double, String, or bytes.
final class QueryResult {
  const QueryResult(this.columns, this.rows);

  final List<String> columns;
  final List<List<Object?>> rows;
}

/// A native call stopped. [code] matches the C header.
final class DbmSqliteException implements Exception {
  const DbmSqliteException(this.code, this.message);

  final int code;
  final String message;

  @override
  String toString() {
    return message;
  }
}
