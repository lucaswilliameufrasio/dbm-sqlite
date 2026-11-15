# dbm_sqlite

Flutter plugin for Android and iOS. `flutter pub get` downloads the prebuilt
`libdbm_sqlite.so` files and the iOS `DbmSqlite.xcframework`. The app build
does not run Cargo, and the app developer does not install Rust.

SQL stays in the app. Pass it to `open`. Changing that text does not rebuild
the native library. Checksums published with the package are in
`checksums.sha256`.

Flutter desktop, Flutter web, Expo, and React Native are not part of this
package. The iOS binary is arm64 device and arm64 simulator.

```yaml
dependencies:
  dbm_sqlite:
    git:
      url: https://github.com/lucaswilliameufrasio/dbm-sqlite
      path: flutter/dbm_sqlite
```

```dart
final sql = await rootBundle.loadString('assets/20261115000000000001.sql');
final database = DbmDatabase.open(
  path: path,
  migrations: [
    Migration(version: '20261115000000000001', sql: sql),
  ],
);
```

`open` blocks the isolate that calls it. The returned value owns the only
writable connection. A background isolate uses `DbmDatabase.fromHandle` with
`database.handle`. It does not open the file. The parent directory must
already exist. Close the owner when the app is done with the file.

Shipped apps move forward. This package has no down migration.
