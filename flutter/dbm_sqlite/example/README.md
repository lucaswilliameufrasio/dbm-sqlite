# Caderno de gastos

O app registra um gasto no aparelho. Os dois arquivos em `assets/` são o SQL de subida, os mesmos bytes de `database-migrator/examples/local-ledger/phone`. A abertura aplica os dois. Um arquivo que só tinha a primeira versão conserva a linha e recebe `category` `geral`.

O build não chama o Cargo. O plugin `dbm_sqlite` traz os binários de Android e
iOS. Há também um binário experimental para Linux x86_64. Em 2026-11-20, neste
host x86_64 (CachyOS, glibc 2.44, Flutter 3.50.0), `flutter test -d linux` sob
Xvfb passou a jornada do ledger (2), a variante padrão (1) e a jornada
SQLCipher (1). Isso não é produção e não cobre Linux ARM. Web e os outros
desktops ficam de fora. Nada é publicado no pub.dev.

O diretório `linux/` foi gerado pelo Flutter 3.50.0. `make build-linux`
compila o app, `make run-linux` o inicia e
`make test-integration DEVICE=linux` executa a jornada.

```sh
flutter test
```

No Linux o teste precisa de `DBM_SQLITE_LIB` apontando para `libdbm_sqlite_flutter.so` do host.

## Android e iOS

O Makefile usa o Flutter disponível no `PATH`; substitua com
`make FLUTTER=/caminho/para/flutter <alvo>` se necessário. Execute `make get`,
`make doctor` e `make devices` para preparar e inspecionar o ambiente.

- `make build-android` gera o APK de debug.
- `make test-integration DEVICE=<id>` executa no dispositivo escolhido a
  atualização de uma instalação antiga, o cadastro pela interface, a
  reabertura do banco e a rejeição de SQL alterado sem perda dos dados.
- No Android, `make test-sqlcipher DEVICE=<id>` testa a biblioteca cifrada e
  `make test-sqlite-only DEVICE=<id>` testa a ausência de fallback na padrão.
  A seleção usa `--dart-define=DBM_SQLITE_SQLCIPHER=true`; o app normal ainda
  usa `open` sem cifra, mesmo quando essa variante é selecionada.
- No macOS com Xcode, `make build-ios-simulator` compila para simulador sem
  assinatura. Inicie um simulador e use `make test-integration DEVICE=<id>`.
  `make build-ios-device` compila sem assinatura para aparelho; instalação em
  aparelho físico exige configurar assinatura no Xcode.
- `make run-android DEVICE=<id>` e `make run-ios DEVICE=<id>` abrem o app.

Os testes de integração usam diretórios temporários próprios e não alteram o
banco normal do aplicativo. Executá-los no host não comprova o funcionamento
nos binários Android ou iOS.

Em 2026-11-18, a jornada de integração passou duas vezes no emulador Android
15 (API 35, x86_64), com Flutter 3.50.0: dois testes em cada execução.
Na continuação, a jornada SQLCipher, o teste da variante padrão e os dois
testes SQLite também passaram no mesmo emulador com ambas as bibliotecas no APK.
Em 2026-11-21, no Mac mini, um emulador headless arm64-v8a (Android 15, API 35,
página de 4096 bytes) passou a jornada do ledger (2) e a variante padrão (1).
A jornada SQLCipher fez a abertura cifrada, rejeitou a chave errada e mostrou
o gasto novo na captura; o localizador de texto "Mercado" falhou nessa execução
e também numa repetição posterior da jornada simples. `armeabi-v7a` e página de
16 KB não foram executados.
