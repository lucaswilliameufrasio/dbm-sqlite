# ADR-003: Um dono da conexão

- **Status:** Aceita em 2026-11-15, no corte C4.
- **Data:** 2026-11-15

## Contexto

Um isolate de fundo ou um worker que abre o mesmo arquivo cria outra conexão
e pode escrever enquanto a primeira ainda migra. Se o processo morre no meio,
o próximo a abrir não pode continuar aquele SQL.

A ADR-001 já grava `started` antes do SQL e bloqueia a abertura seguinte. Este
corte impede o segundo abridor enquanto o primeiro ainda está vivo.

## Decisão

- `Database::open` migra e passa a ser o dono da única conexão de escrita.
  Um clone pede essa mesma conexão. Não abre outra.
- `connection` empresta a conexão na thread que chama. O empréstimo termina
  quando o guard cai. Outra thread espera.
- `Database::call(path, ...)` é o pedido do worker ou do isolate. Não aplica
  migration e não cria arquivo. Sem dono vivo, devolve `NoOwner`.
- Um segundo `open` do mesmo arquivo, neste processo ou em outro, devolve
  `Owned` e não escreve. O bloqueio entre processos é um arquivo ao lado,
  `caminho.dbm-owner`, com trava exclusiva. A trava solta quando o último
  dono cai ou quando o processo morre.
- A conexão continua `SQLITE_OPEN_NO_MUTEX`. Quem serializa é o dono, não o
  SQLite compartilhado entre threads.
- `doctor` continua só leitura e não toma a trava.
- Não há `down` nem reconciliação.

## Consequências

Quem quer escrever pede ao dono. Processo morto no meio deixa `started` no
journal, e a abertura seguinte continua bloqueada pela ADR-001. O arquivo
`.dbm-owner` pode permanecer vazio no diretório depois que o dono fecha.
