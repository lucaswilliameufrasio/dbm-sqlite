# ADR-007: Flutter desktop no macOS arm64

- **Status:** Experimental. A verificação desta execução está abaixo. Não é suporte de produção.
- **Data:** 2026-11-20

## Contexto

A ADR-004 deixou desktop de fora e a ADR-005 fez o primeiro corte em Linux
x86_64. macOS exige binário e validação próprios. O app Flutter continua sem
chamar o Cargo: quem altera o SQL não recompila a biblioteca nativa.

## Decisão

O pacote vende dois XCFrameworks macOS arm64, um SQLite e um SQLCipher,
produzidos por `tools/vendor-macos-prebuilts.sh` com o toolchain Rust 1.99.1.
O podspec apenas aponta esses frameworks. O mínimo do sistema é 12.0, igual ao
runner do Flutter 3.50. Não há slice Intel. A seleção é a mesma das outras
plataformas: sem `--dart-define=DBM_SQLITE_SQLCIPHER=true` o app abre SQLite;
com a definição, abre o framework cifrado. `open` continua em texto claro.
`openEncrypted` não recorre a texto claro. Windows, web e macOS Intel ficam de
fora. A jornada do app Flutter em Linux x86_64 continua por verificar.

## Verificação

No Mac Apple Silicon com macOS 27.2, Xcode 27.2 e Flutter 3.50.0, os dois
XCFrameworks arm64 foram gerados por `tools/vendor-macos-prebuilts.sh` com Rust
1.99.1. `flutter build macos --debug` do exemplo produziu `local_ledger.app`
com `DbmSqlite.framework` e `DbmSqliteCipher.framework` versionados. Essa
compilação não chamou o Cargo.

No mesmo host, ainda com Flutter 3.50.0:

- Testes do pacote, em processos separados: SQLite com `DBM_SQLITE_LIB` no
  binário plano (`flutter test`, 5 passaram e 2 foram pulados); SQLCipher com
  `DBM_TEST_SQLCIPHER=1` e `--dart-define=DBM_SQLITE_SQLCIPHER=true`
  (`encryption_test.dart`, 3 passaram); biblioteca ausente com
  `DBM_TEST_MISSING_CIPHER=1` (`missing_cipher_test.dart`, 1 passou).
- `flutter test integration_test/ledger_test.dart -d macos`: 2 passaram.
- `flutter test integration_test/sqlcipher_test.dart -d macos`: a variante
  padrão recusou `openEncrypted` e não criou arquivo. 1 passou.
- `flutter test --dart-define=DBM_SQLITE_SQLCIPHER=true integration_test/sqlcipher_test.dart -d macos`:
  jornada cifrada com chave sintética. 1 passou.

`path_provider` no macOS devolve Caches mais o bundle id e não cria esse
diretório. Os testes de integração passam a criá-lo antes de `createTemp`.
`flutter run -d macos` subiu `lib/main.dart`. O processo de debug ficou no ar
e criou `Documents/ledger.sqlite` com cabeçalho `SQLite format 3`, em texto
claro. Cada `flutter test`, e também esse `flutter run`, imprimiu
`Failed to foreground app; open returned 1` no attach e mesmo assim executou.
Um `open` posterior do mesmo bundle devolveu 0. O aviso de Swift Package Manager para macOS
continua, sem interromper o build. Não há slice Intel. Não é produção.
Aparelho físico, assinatura de distribuição, custódia, rotação e backup
cifrado ficaram de fora. A jornada do app Flutter em Linux x86_64 continua
por verificar.
