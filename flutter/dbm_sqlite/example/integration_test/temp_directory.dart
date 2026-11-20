import 'dart:io';

import 'package:path_provider/path_provider.dart';

/// New directory under [getTemporaryDirectory].
///
/// On macOS that path is the Caches directory plus the bundle id.
/// path_provider does not create it, unlike the application-support
/// directory, so [Directory.createTemp] fails until the parent exists.
Future<Directory> integrationTemp(String prefix) async {
  final root = await getTemporaryDirectory();
  await root.create(recursive: true);
  return root.createTemp(prefix);
}
