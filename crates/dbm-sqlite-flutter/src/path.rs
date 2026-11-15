use std::path::{Path, PathBuf};

/// dbm-sqlite refuses a path that goes through a symlink. On Apple, `/var` is one.
pub(crate) fn resolved_database_path(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    let name = path
        .file_name()
        .ok_or_else(|| "path has no file name".to_string())?;
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let parent = parent
        .canonicalize()
        .map_err(|err| format!("failed to resolve {}: {err}", parent.display()))?;
    let resolved = parent.join(name);
    if resolved.to_str().is_none() {
        return Err("resolved path is not unicode".to_string());
    }
    Ok(resolved)
}
