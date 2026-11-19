use dbm_sqlite::{Database, Migration};
use rusqlite::{Connection, params};

#[test]
fn public_connection_uses_the_same_rusqlite_type_as_a_consumer() {
    let directory = std::env::temp_dir().join(format!(
        "dbm-public-rusqlite-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir(&directory).expect("create isolated directory");
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    let migrations = [Migration {
        version: "20261118000000000001",
        sql: "CREATE TABLE sample (value TEXT NOT NULL);",
    }];
    let database = Database::open(directory.join("test.sqlite"), &migrations).expect("open");
    let connection = database.connection().expect("connection");
    let connection: &Connection = &connection;
    connection
        .execute("INSERT INTO sample VALUES (?1)", params!["synthetic"])
        .expect("insert");
    let value: String = connection
        .query_row("SELECT value FROM sample", [], |row| row.get(0))
        .expect("query");
    assert_eq!(value, "synthetic");
}
