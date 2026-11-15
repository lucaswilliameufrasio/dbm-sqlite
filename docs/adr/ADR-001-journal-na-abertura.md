# ADR-001: Journal na abertura

- **Status:** Aceita em 2026-11-15, no corte C2.
- **Data:** 2026-11-15

## Contexto

A biblioteca abre o SQLite do aplicativo antes de qualquer SQL de migration.
Ela copia as invariantes do journal do `dbm`: uma versão aplicada não muda de
conteúdo, checksum divergente fica visível, execução desconhecida bloqueia
escrita. Não usa a metadata 5 nem aprovação de produção.

## Decisão

- A tabela `dbm_sqlite_journal` guarda `version`, `checksum` e `state`.
  `version` é o timestamp UTC de 20 dígitos. `checksum` é `sha256:raw:v1` dos
  bytes exatos do SQL. `state` é `started`, `applied` ou `failed`.
- `open` lê o journal antes de escrever SQL de migration. Arquivo ausente é
  criado. Symlink não é seguido. Diretório pai ausente não é criado.
- Versão `applied` com outros bytes é erro de checksum e não escreve.
  Versão `applied` ausente da lista também não escreve. Estado `started` ou
  desconhecido bloqueia escrita e não reaplica o SQL. Estado `failed` só
  tenta de novo quando a mesma versão vem na lista.
- O `started` é confirmado antes do SQL. O SQL e a passagem para `applied`
  ficam na mesma transação. Falha de SQL faz rollback e grava `failed`.
  O arquivo que já existia permanece; o efeito da migration que falhou não
  fica gravado.
- Não há `down` neste corte. Aplicativos publicados seguem só para a frente.

## Consequências

Um crash depois do `started` e antes do `applied` exige uma decisão posterior
para desbloquear o arquivo. Essa decisão não entra neste corte. O formato não
é a metadata 5 e não deve ser lido como se fosse.
