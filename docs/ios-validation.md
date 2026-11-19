# Preparação e validação iOS no Mac

Este fluxo é experimental e ainda não foi executado. O checkout atual contém o
XCFramework SQLite anterior à atualização de `rusqlite`; o framework SQLCipher
não está incluído. Não use este pacote como suporte iOS validado nem em produção
antes de compilar e testar. Não há conversão de banco, custódia ou rotação de
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

Cada comando remove e recria seu próprio diretório `target/ios-xcframework`
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
não sobrescreva trabalho existente. O backup não deve ser publicado. O podspec
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
