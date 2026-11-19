import 'dart:io';
import 'dart:typed_data';

import 'package:dbm_sqlite/dbm_sqlite.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:path_provider/path_provider.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('missing selected cipher framework cannot create plaintext', (
    tester,
  ) async {
    expect(Platform.isIOS, isTrue);
    expect(const bool.fromEnvironment('DBM_SQLITE_SQLCIPHER'), isTrue);
    final root = await getTemporaryDirectory();
    final directory = await root.createTemp('dbm-ios-missing-cipher-');
    addTearDown(() async {
      await directory.delete(recursive: true);
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
