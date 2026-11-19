FLUTTER ?= flutter
DEVICE ?=
DEVICE_FLAG = $(if $(DEVICE),-d $(DEVICE),)

.PHONY: help doctor devices get analyze test test-integration test-sqlcipher \
	test-sqlite-only build-android \
	build-ios-simulator build-ios-device build-linux run-android run-ios run-linux

help:
	@printf '%s\n' \
	  'make doctor                 Verifica as ferramentas Flutter' \
	  'make devices                Lista dispositivos e simuladores' \
	  'make get                    Instala dependências do projeto' \
	  'make analyze                Analisa o código Dart' \
	  'make test                   Executa testes locais (Linux: DBM_SQLITE_LIB)' \
	  'make test-integration DEVICE=<id>  Executa jornada no Android ou iOS' \
	  'make test-sqlcipher DEVICE=<id>    Testa variante SQLCipher Android' \
	  'make test-sqlite-only DEVICE=<id>  Confere que SQLite não cifra por fallback' \
	  'make build-android          Gera APK debug' \
	  'make build-ios-simulator    Compila iOS para simulador, sem assinatura' \
	  'make build-ios-device       Compila iOS para aparelho, sem assinatura' \
	  'make build-linux            Compila o app Linux x86_64 experimental' \
	  'make run-linux              Executa o app Linux experimental' \
	  'make run-android DEVICE=<id>  Executa no Android' \
	  'make run-ios DEVICE=<id>    Executa no iOS'

doctor:
	$(FLUTTER) doctor -v

devices:
	$(FLUTTER) devices

get:
	$(FLUTTER) pub get

analyze:
	$(FLUTTER) analyze

test:
	$(FLUTTER) test test

test-integration:
	@test -n "$(DEVICE)" || { printf '%s\n' 'Informe DEVICE=<id> de flutter devices'; exit 1; }
	$(FLUTTER) test integration_test/ledger_test.dart $(DEVICE_FLAG)

test-sqlcipher:
	@test -n "$(DEVICE)" || { printf '%s\n' 'Informe DEVICE=<id> de flutter devices'; exit 1; }
	$(FLUTTER) test --dart-define=DBM_SQLITE_SQLCIPHER=true integration_test/sqlcipher_test.dart $(DEVICE_FLAG)

test-sqlite-only:
	@test -n "$(DEVICE)" || { printf '%s\n' 'Informe DEVICE=<id> de flutter devices'; exit 1; }
	$(FLUTTER) test integration_test/sqlcipher_test.dart $(DEVICE_FLAG)

build-android:
	$(FLUTTER) build apk --debug

build-ios-simulator:
	$(FLUTTER) build ios --debug --simulator --no-codesign

build-ios-device:
	$(FLUTTER) build ios --debug --no-codesign

build-linux:
	$(FLUTTER) build linux --debug

run-linux:
	$(FLUTTER) run -d linux

run-android run-ios:
	@test -n "$(DEVICE)" || { printf '%s\n' 'Informe DEVICE=<id> de flutter devices'; exit 1; }
	$(FLUTTER) run $(DEVICE_FLAG)
