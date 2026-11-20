# Preparação e validação iOS no Mac

Este fluxo é experimental. Ele foi executado uma vez em 2026-11-20 no Mac Apple
Silicon, com o código Rust de `806d93e26037e1d45c980bead0013abcc139ebfe` e com
este script de montagem já corrigido para o bash 3.2. Os XCFrameworks vendored
são a saída dessa execução; o registro está na seção final. O resultado cobre o
simulador arm64 descrito abaixo. Aparelho físico, assinatura, distribuição e uso
em produção continuam fora deste registro. Não há conversão de banco, custódia ou rotação de
chaves. `open` continua sem cifra; use `openEncrypted` para arquivos cifrados.

## Pré-requisitos

No Mac Apple Silicon, obtenha o mesmo checkout incluindo estas alterações.
São necessários Xcode selecionado por `xcode-select`, SDKs iOS e simulador,
Rust/rustup com toolchain 1.99.1, Flutter e CocoaPods compatíveis com o projeto.
Confira `xcodebuild -version`, `xcrun --sdk iphoneos --show-sdk-version`,
`xcrun --sdk iphonesimulator --show-sdk-version`, `rustup show`,
`flutter doctor -v` e `pod --version`. Os scripts adicionam os targets Rust
`aarch64-apple-ios` e `aarch64-apple-ios-sim` e podem baixar dependências.
Não forneça chaves reais aos testes. Não é necessário assinar para compilar
para simulador; aparelho físico requer configuração de assinatura no Xcode.
Confira também `ruby -c flutter/dbm_sqlite/ios/dbm_sqlite.podspec` na raiz.

## Montagem e inspeção

Na raiz de `dbm-sqlite`, execute:

```sh
bash tools/assemble-ios-xcframework.sh plain
bash tools/assemble-ios-xcframework.sh sqlcipher
```

No macOS, `/bin/bash` 3.2 trata a expansão de um array vazio sob `set -u` como
variável inexistente. A variante plain não passa features Cargo; o script evita
essa expansão. Cada comando remove e recria seu próprio diretório `target/ios-xcframework`
ou `target/ios-xcframework-cipher`, respectivamente; preserve previamente
qualquer arquivo próprio nesses diretórios. A saída fica nesses diretórios, não
substitui o pacote automaticamente. Os frameworks têm identificadores, nomes
e install names distintos. Ambos têm device arm64 iOS 13 e simulador arm64
iOS 14; simulador x86_64 não está incluído.

Inspecione `Info.plist` de cada XCFramework, as arquiteturas com `lipo -info`
e os install names com `otool -D` em cada executável. Use `nm -gU` nos quatro
executáveis: `dbm_sqlite_open_encrypted` e `dbm_sqlite_doctor_encrypted`
devem existir apenas nos dois executáveis `DbmSqliteCipher`. Verifique também
`otool -L` e ausência de dependências inesperadas. Pare se qualquer inspeção
falhar; a presença de símbolos não prova isolamento ou funcionamento.

Após inspecionar, preserve o framework atual e instale as saídas no checkout:

```sh
mv flutter/dbm_sqlite/ios/Frameworks/DbmSqlite.xcframework flutter/dbm_sqlite/ios/Frameworks/DbmSqlite.xcframework.before-ios-rebuild
cp -R target/ios-xcframework/DbmSqlite.xcframework flutter/dbm_sqlite/ios/Frameworks/
cp -R target/ios-xcframework-cipher/DbmSqliteCipher.xcframework flutter/dbm_sqlite/ios/Frameworks/
bash tools/hash-flutter-prebuilts.sh
```

Antes de copiar, confirme que os destinos de backup e SQLCipher não existem;
não sobrescreva trabalho existente. O backup não deve ser publicado; `*.before-ios-rebuild/` está no `.gitignore`. O podspec
inclui o framework cifrado somente quando presente; selecionar SQLCipher sem
ele deve falhar ao carregar, nunca abrir SQLite como fallback.

## Jornada no simulador

Inicie um simulador arm64, confira seu identificador com `flutter devices`, e,
em `flutter/dbm_sqlite/example`, execute:

```sh
make get
make build-ios-simulator
make test-sqlite-only DEVICE=<id-do-simulador>
make test-integration DEVICE=<id-do-simulador>
make test-sqlcipher DEVICE=<id-do-simulador>
```

Substitua `<id-do-simulador>` pelo ID real, sem os sinais `<` e `>`. A jornada
cifrada usa chave sintética e verifica upgrade, interface, diagnóstico, chave
incorreta e preservação, além de reabertura. A jornada padrão verifica que
`openEncrypted` falha quando a variante padrão foi selecionada. Confira
também no app gerado que ambos os frameworks foram embutidos, que os símbolos
não se confundem no processo e que a seleção cifrada falha sem criar banco
quando o framework cifrado não estiver presente. Teste essa ausência em um
checkout ou pacote de teste isolado, não removendo arquivos de outros projetos.
Nesse checkout isolado, sem `DbmSqliteCipher.xcframework`, recrie os pods e
execute, no diretório do exemplo:

```sh
flutter test --dart-define=DBM_SQLITE_SQLCIPHER=true integration_test/missing_cipher_test.dart -d <id-do-simulador>
```

Se necessário, regenere os pods após trocar frameworks; não use um build em
cache como evidência.

Registre commit exato, macOS/Xcode/SDK/Flutter, modelo e versão do simulador,
arquiteturas, comandos e resultados. Uma execução em simulador não valida
aparelho físico, assinatura, distribuição, backups ou segurança criptográfica.
`make build-ios-device` apenas compila sem assinatura. Nenhum destes comandos
publica o aplicativo; não configure certificados nem faça upload como parte
deste procedimento.

## Execução registrada

Em 2026-11-20, no Mac Apple Silicon:

- macOS 27.2 (26B123), Xcode 27.2 (27B52), SDKs iphoneos e iphonesimulator 27.2
- Rust 1.99.1, Flutter 3.50.0, Dart 3.14.0, CocoaPods 1.16.2
- simulador iPhone 18 Pro Max, iOS 27.0, já iniciado
- código Rust em `806d93e26037e1d45c980bead0013abcc139ebfe`

`simctl spawn` de `uname` no simulador falhou com `LaunchdSimError 111`, então a
arquitetura de convidado não foi lida por esse comando. Os quatro executáveis
inspecionados são arm64.

Montagem, em sequência: `bash tools/assemble-ios-xcframework.sh plain` e
`bash tools/assemble-ios-xcframework.sh sqlcipher`. As duas concluíram. Cada
XCFramework tem device arm64 com mínimo iOS 13.0 e simulador arm64 com mínimo
iOS 14.0, SDK 27.2. `otool -L` mostrou o próprio framework,
`/usr/lib/libSystem.B.dylib` e `/usr/lib/libiconv.2.dylib`.
`dbm_sqlite_open_encrypted` e `dbm_sqlite_doctor_encrypted` aparecem só nos dois
executáveis `DbmSqliteCipher`.

Empacotamento: o framework anterior foi movido para
`DbmSqlite.xcframework.before-ios-rebuild` e fica fora do Git. As saídas foram
copiadas para `flutter/dbm_sqlite/ios/Frameworks/` e
`tools/hash-flutter-prebuilts.sh` reescreveu `checksums.sha256`.

Execução em `flutter/dbm_sqlite/example`, com Flutter 3.50.0, no simulador
acima. `make build-ios-simulator` gerou `Runner.app`. Em seguida, com saída 0:

- `make test-sqlite-only`: a variante padrão recusou `openEncrypted` e não criou
  arquivo. 1 teste.
- `make test-integration`: upgrade com despesa nova e rejeição de migration
  alterada sem perder linhas. 2 testes.
- `make test-sqlcipher`: jornada cifrada com chave sintética. 1 teste.
- checkout isolado sem `DbmSqliteCipher.xcframework` e com
  `DBM_SQLITE_SQLCIPHER=true`: `missing_cipher_test` recusou a abertura e não
  criou arquivo em texto claro. 1 teste.

O `Runner.app` embutiu os dois frameworks arm64, com install names distintos. A
assinatura era ad hoc, sem team. A evidência de execução são esses testes.
Compilação e execução no macOS desktop, aparelho físico, assinatura de
distribuição, custódia de chave, rotação e backup cifrado não fizeram parte
desta execução.
