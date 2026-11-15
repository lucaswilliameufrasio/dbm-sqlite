use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use dbm_sqlite::Database;

struct Handles {
    next: u64,
    items: HashMap<u64, Database>,
}

static HANDLES: LazyLock<Mutex<Handles>> = LazyLock::new(|| {
    Mutex::new(Handles {
        next: 1,
        items: HashMap::new(),
    })
});

fn lock() -> Result<std::sync::MutexGuard<'static, Handles>, String> {
    HANDLES
        .lock()
        .map_err(|_| "native handle table stopped".to_string())
}

pub(crate) fn insert(database: Database) -> Result<u64, String> {
    let mut handles = lock()?;
    if handles.next == 0 {
        return Err("native handle table is full".to_string());
    }
    let id = handles.next;
    handles.next = handles.next.wrapping_add(1);
    if handles.next == 0 {
        handles.next = 1;
    }
    handles.items.insert(id, database);
    Ok(id)
}

pub(crate) fn clone_database(id: u64) -> Result<Database, String> {
    let handles = lock()?;
    handles
        .items
        .get(&id)
        .cloned()
        .ok_or_else(|| "unknown database handle".to_string())
}

pub(crate) fn remove(id: u64) -> Result<(), String> {
    let mut handles = lock()?;
    if handles.items.remove(&id).is_none() {
        return Err("unknown database handle".to_string());
    }
    Ok(())
}
