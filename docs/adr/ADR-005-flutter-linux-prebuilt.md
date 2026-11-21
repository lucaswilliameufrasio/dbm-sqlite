# ADR-005: Primeiro corte Flutter desktop em Linux x86_64

- **Status:** Em implementação; suporte ainda não declarado.
- **Data:** 2026-11-18

## Contexto

O responsável pediu Flutter desktop, web, sync e SQLCipher, nesta ordem antes
de React Native/Expo. A ADR-004 excluía desktop e exigia binários prontos:
compilar o app ou alterar o SQL não deve executar Cargo. O host disponível
permite testar a ABI Linux x86_64, mas não possui o toolchain gráfico Flutter
completo. macOS e Windows exigem seus próprios binários e validação nativa.

## Corte

O primeiro corte prepara o plugin Linux x86_64 com a mesma ABI C, journal e
regras de dono já usados pelo Android e iOS. O mantenedor produz a biblioteca
em Debian Bookworm com Rust fixado; o pacote Flutter a inclui e confere seu
checksum. CMake só empacota o arquivo existente e recusa outras arquiteturas,
sem compilar Rust. O usuário não precisa de Cargo ao construir o aplicativo.

Usar o arquivo host via `DBM_SQLITE_LIB` é útil para testes, mas não substitui
um binário incluído no aplicativo. Compilar via hook foi descartado por mudar
o contrato da ADR-004. Web não usa `dart:ffi` nativo e requer outra decisão
de persistência e concorrência; não é implementada por este corte.

## Verificação e limites

São necessários checks de checksum e formato ELF, testes da ABI real e uma
jornada de app Flutter em Linux. A arquitetura, glibc e plataforma efetivamente
testadas devem ser explicitadas. A jornada nativa passou neste host, e o corte
continua experimental: não é suporte desktop completo e não demonstra macOS,
Windows ou Linux ARM. Quando um desses alvos for adicionado, suas bibliotecas
e testes deverão ser específicos da plataforma.

O exemplo Flutter possui um runner Linux gerado com Flutter 3.50.0 e os alvos
`make build-linux` e `make run-linux`. Em 2026-11-20, num host x86_64 CachyOS
com glibc 2.44 e Flutter 3.50.0, `flutter test -d linux` sob Xvfb passou a
jornada do ledger (2 testes), a variante SQLite padrão (1) e a jornada
SQLCipher com chave sintética (1). O bundle de debug continha as duas
bibliotecas vendidas. O display era virtual. No mesmo host, em 2026-11-20, uma
sessão Xvfb privada mostrou a janela do exemplo e a jornada sintética na tela.
Uma sessão KWin privada desenhou só o quarto inferior esquerdo da superfície
Flutter e o clique não gravou gasto.

`tools/check-flutter-plugin.sh` também configura o CMake em um projeto
descartável: exige que x86_64 forneça duas bibliotecas existentes e
que aarch64 seja recusado. Um segundo teste instala a biblioteca em um bundle
descartável com os mesmos caminhos e RPATH do template Flutter e usa uma
biblioteca carregadora para abrir, escrever e reabrir um SQLite pela ABI C,
sem `LD_LIBRARY_PATH`. Também verifica falha sem criar banco quando a
biblioteca está ausente. Esse executável é um simulador do empacotamento; o
teste não abre uma janela nem confirma o bundle final de um app Flutter real.
