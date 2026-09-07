//! Filesystem-only package validation. Never imports modules or executes scripts.
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::contract::{MAX_ARTIFACT_BYTES, MAX_PACKAGE_BYTES, MAX_PACKAGE_FILES, validate_id};
use crate::{PluginError, PluginManifest, Result};

pub fn confined_file(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if relative.is_empty()
        || relative.contains('\\')
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(PluginError::validation_error(
            "path",
            "Expected a confined relative file path",
        ));
    }
    let root = root.canonicalize()?;
    let mut current = root.clone();
    for component in path.components() {
        current.push(component);
        if std::fs::symlink_metadata(&current)?
            .file_type()
            .is_symlink()
        {
            return Err(PluginError::validation_error(
                "path",
                "Package symlinks are not allowed",
            ));
        }
    }
    let resolved = current.canonicalize()?;
    if !resolved.starts_with(&root) || !resolved.is_file() {
        return Err(PluginError::validation_error(
            "path",
            "Package file escapes its root",
        ));
    }
    Ok(resolved)
}

pub fn destination(root: &Path, id: &str) -> Result<PathBuf> {
    validate_id(id)?;
    std::fs::create_dir_all(root)?;
    let root = root.canonicalize()?;
    let path = root.join(id);
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return Err(PluginError::validation_error(
                "destination",
                "Expected a real package directory",
            ));
        }
    }
    Ok(path)
}

pub fn validate_package(root: &Path) -> Result<PluginManifest> {
    let manifest_path = confined_file(root, crate::MANIFEST_FILE)?;
    if manifest_path.metadata()?.len() > 256 * 1024 {
        return Err(PluginError::validation_error(
            "manifest",
            "Manifest exceeds 256 KiB",
        ));
    }
    let manifest = PluginManifest::from_file(manifest_path)?;
    manifest.validate()?;
    let artifact = confined_file(root, &manifest.runtime.entrypoint)?;
    let size = artifact.metadata()?.len();
    if size == 0 || size > MAX_ARTIFACT_BYTES {
        return Err(PluginError::validation_error(
            "artifact",
            "Artifact must contain 1 byte to 16 MiB",
        ));
    }
    package_files(root)?;
    Ok(manifest)
}

pub fn package_files(root: &Path) -> Result<Vec<PathBuf>> {
    let root = root.canonicalize()?;
    let mut files = Vec::new();
    let mut budget = 0;
    collect(&root, &root, &mut files, &mut budget, 0)?;
    files.sort();
    Ok(files)
}

fn collect(
    root: &Path,
    dir: &Path,
    files: &mut Vec<PathBuf>,
    bytes: &mut u64,
    depth: usize,
) -> Result<()> {
    if depth > 16 {
        return Err(PluginError::validation_error(
            "package",
            "Package nesting exceeds 16",
        ));
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let name = entry.file_name();
        if [".git", "target", "node_modules"]
            .iter()
            .any(|n| name == *n)
        {
            continue;
        }
        if kind.is_symlink() || (!kind.is_dir() && !kind.is_file()) {
            return Err(PluginError::validation_error(
                "package",
                "Links and special files are not supported",
            ));
        }
        if kind.is_dir() {
            collect(root, &entry.path(), files, bytes, depth + 1)?;
        } else {
            *bytes += entry.metadata()?.len();
            files.push(
                entry
                    .path()
                    .strip_prefix(root)
                    .map_err(|_| PluginError::validation_error("package", "File outside package"))?
                    .to_path_buf(),
            );
            if files.len() > MAX_PACKAGE_FILES || *bytes > MAX_PACKAGE_BYTES {
                return Err(PluginError::validation_error(
                    "package",
                    "Package exceeds file or byte limits",
                ));
            }
        }
    }
    Ok(())
}

/// Trust pins every packaged file, not just the entry module.
pub fn fingerprint(root: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    for path in package_files(root)? {
        let name = path.to_string_lossy();
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        let data = std::fs::read(root.join(&path))?;
        hash.update((data.len() as u64).to_le_bytes());
        hash.update(data);
    }
    Ok(hex::encode(hash.finalize()))
}

pub fn artifact_hash(path: &Path) -> Result<String> {
    Ok(hex::encode(Sha256::digest(std::fs::read(path)?)))
}
