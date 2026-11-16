# Caderno de gastos

O app registra um gasto no aparelho. Os dois arquivos em `assets/` são o SQL de subida, os mesmos bytes de `database-migrator/examples/local-ledger/phone`. A abertura aplica os dois. Um arquivo que só tinha a primeira versão conserva a linha e recebe `category` `geral`.

O build não chama o Cargo. O plugin `dbm_sqlite` já traz os binários de Android e iOS. Desktop e web ficam de fora. Nada é publicado no pub.dev.

```sh
flutter test
```

No Linux o teste precisa de `DBM_SQLITE_LIB` apontando para `libdbm_sqlite_flutter.so` do host.
