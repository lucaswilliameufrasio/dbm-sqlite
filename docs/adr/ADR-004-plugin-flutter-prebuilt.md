# ADR-004: Plugin Flutter com binário pronto

- **Status:** Aceita em 2026-11-15, no corte C6.
- **Data:** 2026-11-15

## Contexto

O aplicativo Flutter precisa abrir um SQLite, aplicar o SQL que o próprio app
carrega e usar essa conexão. Quem escreve o app não instala Rust. Mudar o SQL
não pode recompilar a biblioteca nativa. O `flutter build` não pode chamar o
Cargo.

O Flutter 3.38 recomenda um `hook/build.dart` que compila o nativo durante o
build. Isso é o contrário deste corte. Desktop e web continuam de fora. Expo e
React Native continuam de fora. Publicar no pub.dev e enviar binário para uma
release do GitHub continuam sem autorização.

A crate `dbm-sqlite` já abre, migra e segura o único dono da conexão. O plugin
não precisa mudar essa API.

## Decisão

- O plugin Dart fica em `flutter/dbm_sqlite`, no mesmo repositório. Não há
  terceiro repositório. O `pubspec` declara só Android e iOS.
- A crate `dbm-sqlite-flutter` expõe a API C por cima de `Database::open`,
  `connection` e `doctor`. O SQL e os argumentos vêm do Dart em cada chamada.
  O caminho é resolvido antes do `open`, porque no Apple `/var` é um symlink
  e o `dbm-sqlite` recusa um caminho que atravessa symlink.
- O mantenedor compila os binários e os coloca dentro do pacote, com
  `checksums.sha256`. `flutter pub get` baixa esses arquivos porque eles fazem
  parte do pacote. O Gradle só empacota `jniLibs`. O podspec só aponta o
  XCFramework. Nenhum dos dois chama o Cargo.
- Android leva `libdbm_sqlite.so` em `arm64-v8a`, `armeabi-v7a` e `x86_64`.
  iOS leva um XCFramework dinâmico com device `arm64` (iOS 13) e simulador
  `arm64` (iOS 14, mínimo dessa arquitetura). Simulador Intel, desktop e web
  ficam de fora.
- O XCFramework é produzido no runner `macos-latest`, porque a máquina de
  desenvolvimento não tem SDK da Apple. O artifact do Actions só transporta
  esse produto até o commit que o vende no pacote. O app não baixa o artifact.
- Um isolate usa o identificador do dono. Não abre outro arquivo.

## Consequências

O app passa o SQL dos assets na abertura. Trocar o asset não troca o `.so`.
O journal continua o da ADR-001, e o segundo `open` continua o da ADR-003.
Regenerar um binário é trabalho do mantenedor, com
`tools/vendor-android-prebuilts.sh` e `tools/assemble-ios-xcframework.sh`, e
os checksums acompanham o arquivo vendido. Não há `down` nem reconciliação.
