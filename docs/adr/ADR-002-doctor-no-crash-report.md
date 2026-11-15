# ADR-002: Doctor curto para crash report

- **Status:** Aceita em 2026-11-15, no corte C3.
- **Data:** 2026-11-15

## Contexto

Quando a abertura falha, o sintoma que chega é o aplicativo que não abre. O
registro que vai no crash report precisa da versão do aplicativo, da cabeça
do journal, do checksum, da versão do SQLite e do estado da execução. Esse
registro também é lido por quem não deveria ver as linhas do usuário.

A ADR-001 já bloqueia escrita quando o journal está inconsistente. Este corte
não decide como desbloquear um `started`.

## Decisão

- `doctor(path, app_version, migrations)` só lê. Não cria arquivo, não cria
  diretório, não segue symlink, não aplica migration e não tenta de novo uma
  linha `failed` ou `started`.
- O texto do relatório leva a versão desta crate, a versão do SQLite embutido,
  a `app_version` quando ela é uma linha de até 128 bytes sem caractere de
  controle, a cabeça do journal e os achados. A cabeça é a maior `version` em
  ordem lexicográfica, com `state` e `checksum`.
- O caminho do arquivo fica de fora. Quem chama já o tem, e um caminho absoluto
  costuma carregar o nome do usuário.
- SQL de migration, nome de tabela e linha de tabela de aplicativo ficam de
  fora. Entram versão, checksum e estado só quando têm a forma do journal:
  versão de 20 dígitos, checksum `sha256:raw:v1` com 64 hex minúsculos, estado
  em letras minúsculas de até 32 caracteres. O resto aparece como `omitted`.
- Achados de catálogo e de journal aparecem juntos. `open` continua parando no
  primeiro bloqueio. `pending` significa que a versão está no catálogo e não
  está no journal; o doctor não a executa.
- Não há reconciliação neste corte.

## Consequências

O relatório pode ser anexado a um crash report sem copiar o banco. Um `started`
continua bloqueando a próxima abertura. A decisão de desbloquear esse arquivo
não entra aqui.
