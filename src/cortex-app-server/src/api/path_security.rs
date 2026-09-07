//! Path traversal protection utilities.
//!
//! Provides functions to validate and sanitize paths for safe file operations,
//! preventing path traversal attacks and restricting access to allowed directories.

use cortex_common::normalize_path as normalize_path_util;

/// Normalizes a path by resolving `.` and `..` components without filesystem access.
pub fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
    normalize_path_util(path)
}

/// Validates that a path is safe for file operations.
/// Prevents path traversal attacks by canonicalizing and checking against allowed roots.
pub fn validate_path_safe(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let root = std::env::current_dir().map_err(|_| "Workspace is unavailable")?;
    validate_in_workspace(path, &root)
}

fn validate_in_workspace(
    path: &std::path::Path,
    root: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|_| "Workspace is unavailable")?;
    let mut ancestor = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let mut missing = Vec::new();
    // Resolve the nearest existing ancestor, including symlinks, before creating
    // any directories. A missing grandparent must not bypass the workspace check.
    while !ancestor.try_exists().map_err(|_| "Cannot inspect path")? {
        if std::fs::symlink_metadata(&ancestor).is_ok() {
            return Err("Unresolved symlink".into());
        }
        let name = ancestor.file_name().ok_or("Invalid path")?.to_os_string();
        missing.push(name);
        if !ancestor.pop() {
            return Err("Invalid path".into());
        }
    }
    let mut canonical = ancestor.canonicalize().map_err(|_| "Cannot resolve path")?;
    if !canonical.starts_with(&root) {
        return Err("Path is outside the workspace".into());
    }
    for component in missing.into_iter().rev() {
        canonical.push(component);
    }
    Ok(canonical)
}

/// Validate a path for write operations (more restrictive).
pub fn validate_path_for_write(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let normalized = normalize_path(path);

    // Forbid writing to system paths
    let forbidden_prefixes: &[&str] = if cfg!(windows) {
        &[
            "C:\\Windows",
            "C:\\Program Files",
            "C:\\Program Files (x86)",
        ]
    } else {
        &[
            "/bin",
            "/sbin",
            "/usr/bin",
            "/usr/sbin",
            "/etc",
            "/var/log",
            "/boot",
        ]
    };

    let path_str = normalized.to_string_lossy().to_lowercase();
    for prefix in forbidden_prefixes {
        if path_str.starts_with(&prefix.to_lowercase()) {
            return Err(format!(
                "Writing to system directory '{}' is not allowed",
                path.display()
            ));
        }
    }

    validate_path_safe(path)
}

/// Validate a path for delete operations (most restrictive).
pub fn validate_path_for_delete(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let validated = validate_path_for_write(path)?;
    let normalized = normalize_path(path);
    let workspace = std::env::current_dir()
        .and_then(|p| p.canonicalize())
        .map_err(|_| "Workspace is unavailable")?;
    if validated == workspace {
        return Err("Cannot delete the workspace root".into());
    }

    // Prevent deletion of home directory
    if let Some(home) = dirs::home_dir()
        && (normalized == home || validated == home)
    {
        return Err("Cannot delete home directory".to_string());
    }

    // Prevent deletion of root directories
    if normalized.parent().is_none() || normalized.as_os_str().is_empty() {
        return Err("Cannot delete root directory".to_string());
    }

    Ok(validated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_existing_and_new_paths_stay_inside_workspace() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(directory.path().join("private"), "outside").unwrap();
        assert!(validate_in_workspace(std::path::Path::new("new/nested/file"), &root).is_ok());
        assert!(validate_in_workspace(std::path::Path::new("../private"), &root).is_err());
        assert!(validate_in_workspace(&directory.path().join("private"), &root).is_err());
        assert!(validate_in_workspace(std::path::Path::new("../new/file"), &root).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn test_symlink_and_missing_descendant_escape_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        std::os::unix::fs::symlink(directory.path(), root.join("escape")).unwrap();
        assert!(validate_in_workspace(std::path::Path::new("escape/new/nested"), &root).is_err());
        std::os::unix::fs::symlink(directory.path().join("missing"), root.join("broken")).unwrap();
        assert!(validate_in_workspace(std::path::Path::new("broken"), &root).is_err());
    }
}
