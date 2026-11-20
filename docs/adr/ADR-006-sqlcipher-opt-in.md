# ADR-006: Abertura SQLCipher opt-in no núcleo Rust

- **Status:** Em implementação; ainda não declarada pronta para produção.
- **Data:** 2026-11-18

O responsável autorizou atualizar `rusqlite` para a versão estável segura
mais recente e adaptar consumidores. A versão `0.43.0` foi identificada no
crates.io em 2026-11-18 e substitui a `0.41.1`. Ela usa `libsqlite3-sys
0.40.0` e `sqlcipher-src 420.0.0`. O `cargo audit` de 2026-11-18 não apontou
vulnerabilidades no lockfile; isso não é uma revisão de segurança completa.
O consumo pela API Rust é incompatível com quem fixa `rusqlite 0.41` em um
grafo que também usa este novo `dbm-sqlite`.

## Contexto e decisão

O responsável pediu SQLCipher. O primeiro recorte é uma API Rust opt-in para
abrir um arquivo novo ou um arquivo já cifrado, recebendo a frase secreta do
aplicativo. O chamador é responsável por obtenção, custódia, duração e limpeza
da chave. Não há fallback para abrir sem chave, conversão de SQLite em texto
claro, rotação nem recuperação de chaves.

A feature `sqlcipher` usa o SQLCipher e o OpenSSL embutidos pelo `rusqlite`.
Features Cargo são aditivas: ativá-la troca o SQLite nativo para todas as
dependências que compartilham `libsqlite3-sys` no mesmo processo. A API padrão
continua sem chave e continua criando bancos SQLite não cifrados. Consumidores
que exigem isolamento entre os dois motores devem compilar processos separados.

A ABI C e o Dart expõem abertura e diagnóstico cifrados somente quando o
binário nativo é compilado com `dbm-sqlite-flutter/sqlcipher`. As funções
cifradas são símbolos opcionais, assim o binário anterior continua utilizável
para SQLite comum. A chave atravessa a ABI como bytes UTF-8 com comprimento;
o Dart limpa sua cópia nativa temporária, mas o chamador conserva a posse dos
seus bytes e nenhum apagamento completo da memória é garantido. Falta do
símbolo é erro explícito, nunca fallback para banco em texto claro.

No corte experimental Linux x86_64 o pacote contém duas bibliotecas com nomes
distintos. O app escolhe SQLCipher em tempo de compilação com a definição Dart
`DBM_SQLITE_SQLCIPHER=true`; por padrão usa SQLite. A escolha é a mesma para
os isolates do aplicativo e não deve ser trocada enquanto houver handles
ativos. O binário SQLCipher também pode abrir arquivos comuns pela API
`open`, mas `openEncrypted` nunca recorre a ela. Bibliotecas carregadas fora
do plugin são responsabilidade do aplicativo; não misture variantes no mesmo
processo. O mesmo corte com dois binários por ABI está incluído para Android;
isso não declara suporte para iOS ou outros desktops.

Antes de ler o journal ou executar SQL, cada conexão aplica a chave e exige que
`cipher_version` exista. Na abertura de arquivo existente isso inclui a
inspeção somente leitura e a conexão de escrita. Frase vazia ou com NUL é
recusada antes de criar o arquivo; erro da aplicação da chave é genérico e não
devolve a frase. Uma chave incorreta ou arquivo incompatível não é reaberto sem
chave. SQLite usa `PRAGMA key` por meio da API segura do `rusqlite`; isso cria
uma cópia temporária da frase no processo, sem garantia de apagamento da memória.

## Limites

Anteriormente, `cargo audit` identificou RUSTSEC-2026-0502 em `sqlcipher-src 419.0.6`,
fixado por `libsqlite3-sys 0.39.1`. A falha pode expor memória quando um
processo abre banco e WAL preparados por terceiro menos confiável. A correção
está em `sqlcipher-src >=420.0.0`; `libsqlite3-sys 0.40.0` já a utiliza, mas
exige atualizar `rusqlite` e sua versão faz parte da API pública porque a
conexão é exposta ao chamador. Essa mudança pode impedir consumidores que
dependem do `rusqlite 0.41` de resolver o grafo por causa do `links=sqlite3`.
Não considerar os testes funcionais uma aprovação de segurança.

`doctor_encrypted` recebe a chave e lê o journal sem aplicar migrations; chave
incorreta e arquivo cifrado inválido recebem o mesmo diagnóstico. Os testes
cobrem arquivo ausente, symlink, chave errada, privacidade do relatório e um
WAL ativo sem mudanças nos bytes do banco e do WAL. SQLite pode manter
arquivos auxiliares de memória compartilhada, que não são cobertos por essa
garantia de bytes. O Dart expõe a API e os binários opt-in Linux x86_64 e
Android e iOS incluem os símbolos cifrados no binário SQLCipher. No iOS esses
símbolos estão só em `DbmSqliteCipher`. Chaves não devem estar em
logs, argumentos de processo ou código de aplicações. O contrato de conversão,
backup, rotação, falhas de energia, plataformas e armazenamento seguro de
chaves exige trabalho e validação próprios antes do uso em produção.

Os binários Flutter de Android, Linux x86_64 e iOS foram recompilados com
`rusqlite 0.43`; o exemplo passou no emulador Android 15 x86_64. No Mac Apple
Silicon, a montagem dos dois XCFrameworks, o empacotamento e as jornadas no
simulador iPhone 18 Pro Max (iOS 27.0) passaram. O registro está em
`docs/ios-validation.md`. Esse registro cobre o simulador. Aparelho físico,
assinatura, compilação no macOS desktop e uso em produção continuam fora dele.
No Android 15 x86_64, a jornada cifrada passou com abertura, migração, UI,
diagnóstico, chave errada e preservação; a variante padrão e a jornada SQLite
também passaram com ambas as bibliotecas empacotadas. Em 2026-11-21, no
emulador arm64-v8a (Android 15, API 35, página de 4096 bytes), a jornada do
ledger e a variante padrão passaram. A jornada SQLCipher abriu o arquivo
cifrado e mostrou o gasto novo; o localizador de texto falhou. `armeabi-v7a`
e página de 16 KB não foram executados.
Uma aplicação de cofre gerada pelo Jirau passou oito testes ao apontar para
este checkout e usar `rusqlite 0.43`; isso não atualiza o commit Git fixado
nos templates distribuídos. A mudança do pin deve ser coordenada com a
disponibilização de um commit da biblioteca.
