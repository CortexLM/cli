use super::*;
use anyhow::Context;
use runtime::activation::Activation;
use std::io::{Read, Write};
use std::path::{Component, Path};
use std::time::Duration;

const REGISTRY: &str = "https://software.cortex.foundation/plugins";

/// Serializes local mutation; never follows a lock or package symlink.
struct InstallLock(PathBuf);
impl InstallLock {
    fn acquire(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        let path = root.join(".install.lock");
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .context("Another plugin operation is running (or left .install.lock after a crash)")?;
        Ok(Self(path))
    }
}
impl Drop for InstallLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Command review a `--json` dry run prints and `--accept-command` pins.
fn command_review(manifest: &runtime::PluginManifest) -> serde_json::Value {
    serde_json::json!({
        "id": manifest.plugin.id,
        "version": manifest.plugin.version,
        "commands": manifest.commands.iter().map(|command| serde_json::json!({
            "name": command.name,
            "aliases": command.aliases,
            "description": command.description,
            "usage": command.usage,
            "args": command.args.iter().map(|arg| serde_json::json!({
                "name": arg.name,
                "required": arg.required,
                "default": arg.default,
            })).collect::<Vec<_>>(),
            "hidden": command.hidden,
        })).collect::<Vec<_>>(),
        "hooks": manifest.hooks.iter().map(|hook| hook.hook_type.to_string()).collect::<Vec<_>>(),
        "tools": manifest.tools.iter().map(|tool| tool.name.clone()).collect::<Vec<_>>(),
        "command_hash": runtime::command_pin::command_hash(manifest),
    })
}

/// Print the review for a package without installing it.
fn review_only(manifest: &runtime::PluginManifest) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&command_review(manifest))?
    );
    Ok(())
}

/// Enforce the reviewed hash and record an accepted pin.
///
/// Fails closed: a mismatch or a malformed hash stops the install before the
/// package reaches the plugin root, and organization policy that requires a
/// pin refuses an install that carries none.
fn enforce_command_pin(
    manifest: &runtime::PluginManifest,
    accept_command: Option<&str>,
    action: &str,
) -> Result<()> {
    let policy = cortex_engine::org_policy::current().plugin_install;
    enforce_command_pin_with(manifest, accept_command, action, policy)
}

/// Policy-explicit form of [`enforce_command_pin`], so the decision can be
/// exercised without a process-global policy directory.
fn enforce_command_pin_with(
    manifest: &runtime::PluginManifest,
    accept_command: Option<&str>,
    action: &str,
    policy: cortex_engine::org_policy::PluginInstallPolicy,
) -> Result<()> {
    match accept_command {
        Some(accepted) => {
            runtime::command_pin::verify_command_hash(manifest, accepted)?;
            audit_command_pin(manifest, accepted, action, policy.as_str())
        }
        None if policy.requires_pin() => bail!(
            "This organization requires --accept-command for plugin installs. Run `cortex plugin {action} {} --json` to review the commands, then pass --accept-command <sha256>.",
            manifest.plugin.id
        ),
        None => Ok(()),
    }
}

/// Record an accepted command pin. Fail closed: a journal write failure
/// aborts the install so an accepted-command decision is never untracked.
fn audit_command_pin(
    manifest: &runtime::PluginManifest,
    accepted: &str,
    action: &str,
    policy: &str,
) -> Result<()> {
    let home = cortex_engine::config::find_cortex_home()
        .unwrap_or_else(|_| std::path::PathBuf::from(".cortex"));
    cortex_engine::audit::record(
        &home,
        cortex_engine::audit::AuditKind::PluginCommandAccepted,
        serde_json::json!({
            "plugin": manifest.plugin.id,
            "version": manifest.plugin.version,
            "accepted_hash": accepted.to_ascii_lowercase(),
            "actual_hash": runtime::command_pin::command_hash(manifest),
            "action": action,
            "policy": policy,
        }),
    )
    .map_err(|error| {
        anyhow::anyhow!(
            "Could not record the accepted command hash in the audit journal: {error}. Install aborted."
        )
    })?;
    Ok(())
}

pub(super) async fn install(args: PluginInstallArgs) -> Result<()> {
    let root = plugins_dir()?;
    let local = Path::new(&args.name);
    let id = if local.exists() {
        if args.json {
            let manifest = inspect_local(local)?;
            return review_only(&manifest);
        }
        install_local(
            &root,
            local,
            args.force,
            args.version.as_deref(),
            None,
            args.accept_command.as_deref(),
            "install",
        )?
    } else {
        runtime::contract::validate_id(&args.name)?;
        let (entry, bytes) = download(&args.name, args.version.as_deref()).await?;
        let source = tempfile::NamedTempFile::new()?;
        std::fs::write(source.path(), bytes)?;
        if args.json {
            let manifest = inspect_archive(source.path())?;
            return review_only(&manifest);
        }
        install_local(
            &root,
            source.path(),
            args.force,
            Some(&entry.version),
            Some(&entry.id),
            args.accept_command.as_deref(),
            "install",
        )?
    };
    if args.trust_code {
        trust(PluginTrustArgs {
            name: id.clone(),
            yes: true,
        })?;
    }
    println!("Installed {id}");
    Ok(())
}

/// Read a package manifest without touching the plugin root.
fn inspect_local(source: &Path) -> Result<runtime::PluginManifest> {
    let stage = tempfile::tempdir()?;
    let package = stage.path().join("package");
    std::fs::create_dir(&package)?;
    if source.is_dir() {
        copy_package(source, &package)?;
        Ok(runtime::package::validate_package(&package)?)
    } else {
        extract(source, &package)?;
        Ok(runtime::package::validate_package(&package_root(
            &package,
        )?)?)
    }
}

/// Read a downloaded archive manifest without touching the plugin root.
fn inspect_archive(source: &Path) -> Result<runtime::PluginManifest> {
    inspect_local(source)
}

pub(super) fn install_local(
    root: &Path,
    source: &Path,
    force: bool,
    version: Option<&str>,
    expected_id: Option<&str>,
    accept_command: Option<&str>,
    action: &str,
) -> Result<String> {
    let _lock = InstallLock::acquire(root)?;
    let root = root.canonicalize()?;
    let stage = tempfile::Builder::new()
        .prefix(".staging-")
        .tempdir_in(&root)?;
    let package = stage.path().join("package");
    std::fs::create_dir(&package)?;
    let candidate = if source.is_dir() {
        copy_package(source, &package)?;
        package
    } else {
        extract(source, &package)?;
        package_root(&package)?
    };
    let manifest = validate::validate(&candidate)?;
    if version.is_some_and(|version| version != manifest.plugin.version)
        || expected_id.is_some_and(|id| id != manifest.plugin.id)
    {
        bail!("Package identity or version does not match the requested plugin");
    }
    // The reviewed hash is checked after the package is validated but before
    // anything is placed in the plugin root.
    enforce_command_pin(&manifest, accept_command, action)?;
    let destination = runtime::package::destination(&root, &manifest.plugin.id)?;
    if destination.exists() && !force {
        bail!("Plugin already installed; use --force");
    }
    if let Err(error) = replace(&candidate, &destination, stage.path()) {
        if stage.path().join("previous").exists() {
            let recovery = stage.keep();
            return Err(error.context(format!(
                "Previous installation retained at {}",
                recovery.display()
            )));
        }
        return Err(error);
    }
    Ok(manifest.plugin.id)
}

fn copy_package(source: &Path, destination: &Path) -> Result<()> {
    for relative in runtime::package::package_files(source)? {
        let input = runtime::package::confined_file(
            source,
            relative.to_str().context("Non-UTF8 package path")?,
        )?;
        let output = destination.join(&relative);
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(input, output)?;
    }
    Ok(())
}

fn replace(candidate: &Path, destination: &Path, stage: &Path) -> Result<()> {
    let backup = stage.join("previous");
    let existed = destination.exists();
    if existed {
        std::fs::rename(destination, &backup)?;
    }
    if let Err(error) = std::fs::rename(candidate, destination) {
        if existed {
            std::fs::rename(&backup, destination).context(
                "Replacement failed and rollback failed; previous package remains in staging",
            )?;
        }
        return Err(error.into());
    }
    Ok(())
}

/// Reject traversal, absolute, backslash, over-deep and duplicate archive paths
/// before any of them is turned into a destination path.
fn check_archive_path(path: &Path, seen: &mut std::collections::HashSet<PathBuf>) -> Result<()> {
    let text = path.to_str().context("Archive path is not UTF-8")?;
    if text.contains('\\')
        || text.is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || path.components().count() > 16
        || !seen.insert(path.to_path_buf())
    {
        bail!("Archive contains an invalid or duplicate path");
    }
    Ok(())
}

fn extract(source: &Path, destination: &Path) -> Result<()> {
    use runtime::contract::{MAX_PACKAGE_BYTES, MAX_PACKAGE_FILES};
    if source.metadata()?.len() > MAX_PACKAGE_BYTES {
        bail!("Compressed package exceeds 64 MiB");
    }
    let decoder = flate2::read::GzDecoder::new(std::fs::File::open(source)?);
    let bounded = decoder.take(MAX_PACKAGE_BYTES + 1);
    let mut archive = tar::Archive::new(bounded);
    let mut bytes = 0_u64;
    let mut seen = std::collections::HashSet::new();
    for (index, entry) in archive.entries()?.enumerate() {
        let mut entry = entry?;
        if index >= MAX_PACKAGE_FILES {
            bail!("Archive exceeds 1024 entries");
        }
        let path = entry.path()?.into_owned();
        check_archive_path(&path, &mut seen)?;
        let kind = entry.header().entry_type();
        if !kind.is_file() && !kind.is_dir() {
            bail!("Archive links and special entries are not allowed");
        }
        bytes = bytes
            .checked_add(entry.size())
            .context("Archive size overflow")?;
        if bytes > MAX_PACKAGE_BYTES {
            bail!("Expanded package exceeds 64 MiB");
        }
        let output = destination.join(&path);
        if kind.is_dir() {
            std::fs::create_dir_all(output)?;
        } else {
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?;
            let copied = std::io::copy(&mut entry, &mut file)?;
            if copied != entry.size() {
                bail!("Archive file was truncated");
            }
        }
    }
    let mut decoder = archive.into_inner();
    let mut tail = Vec::new();
    decoder.read_to_end(&mut tail)?;
    if decoder.limit() == 0 {
        bail!("Expanded archive exceeds 64 MiB including headers");
    }
    Ok(())
}

fn package_root(root: &Path) -> Result<PathBuf> {
    if root.join("plugin.toml").is_file() {
        return Ok(root.to_path_buf());
    }
    let entries: Vec<_> = std::fs::read_dir(root)?.collect::<std::io::Result<_>>()?;
    if entries.len() != 1 || !entries[0].file_type()?.is_dir() {
        bail!("Archive must contain one plugin at root or under its ID");
    }
    let path = entries[0].path();
    let manifest = runtime::PluginManifest::from_file(path.join("plugin.toml"))?;
    if path.file_name().and_then(|s| s.to_str()) != Some(&manifest.plugin.id) {
        bail!("Archive root directory must match the plugin ID");
    }
    Ok(path)
}

pub(super) fn set_enabled(name: &str, enabled: bool) -> Result<()> {
    let root = plugins_dir()?;
    let _lock = InstallLock::acquire(&root)?;
    let path = runtime::package::destination(&root, name)?;
    if !path.is_dir() {
        bail!("Plugin is not installed");
    }
    let config_path = state_path()?;
    let mut activation = Activation::load(Some(&config_path))?;
    if enabled {
        activation.disabled.remove(name);
    } else {
        activation.disabled.insert(name.into());
    }
    activation.save(&config_path)?;
    println!(
        "Plugin {name} {}",
        if enabled { "enabled" } else { "disabled" }
    );
    Ok(())
}

pub(super) fn trust(args: PluginTrustArgs) -> Result<()> {
    if !args.yes {
        bail!(
            "Pass --yes only if you trust this package with native filesystem, network and subprocess access. A Node process is NOT a sandbox"
        );
    }
    let root = plugins_dir()?;
    let _lock = InstallLock::acquire(&root)?;
    let path = runtime::package::destination(&root, &args.name)?;
    validate::validate(&path)?;
    let config_path = state_path()?;
    let mut activation = Activation::load(Some(&config_path))?;
    activation
        .trusted
        .insert(args.name.clone(), runtime::package::fingerprint(&path)?);
    activation.save(&config_path)?;
    println!(
        "Trusted exact package {} for native code execution (NOT sandboxed)",
        args.name
    );
    Ok(())
}

pub(super) fn remove(args: PluginRemoveArgs) -> Result<()> {
    if !args.yes {
        bail!("Removal requires --yes");
    }
    let root = plugins_dir()?;
    let _lock = InstallLock::acquire(&root)?;
    let path = runtime::package::destination(&root, &args.name)?;
    if !path.exists() {
        bail!("Plugin is not installed");
    }
    // Rename inside the owned root before deletion, never delete a user-supplied path.
    let trash = tempfile::Builder::new()
        .prefix(".removing-")
        .tempdir_in(root)?;
    std::fs::rename(path, trash.path().join("package"))?;
    let config_path = state_path()?;
    let mut activation = Activation::load(Some(&config_path))?;
    activation.trusted.remove(&args.name);
    activation.disabled.remove(&args.name);
    activation.save(&config_path)?;
    println!("Removed {}", args.name);
    Ok(())
}

pub(super) async fn list(args: PluginListArgs) -> Result<()> {
    let config = runtime::PluginConfig::default();
    let activation = Activation::load(config.state_path.as_deref())?;
    let manager = runtime::PluginManager::new(config).await?;
    let mut rows = Vec::new();
    for plugin in manager.discover().await {
        let enabled = !activation.disabled.contains(plugin.id());
        if (args.enabled && !enabled) || (args.disabled && enabled) {
            continue;
        }
        rows.push(serde_json::json!({
            "id":plugin.id(),"version":plugin.version(),"path":plugin.path,
            "enabled":enabled,"runtime":plugin.manifest.runtime.kind,
            "trusted":activation.trusted.get(plugin.id()).is_some_and(|hash|
                runtime::package::fingerprint(&plugin.path).is_ok_and(|actual| actual == *hash)),
        }));
    }
    if args.json {
        println!("{}", serde_json::to_string(&rows)?);
    } else {
        for row in rows {
            println!(
                "{} {} enabled={} trusted={}",
                row["id"], row["version"], row["enabled"], row["trusted"]
            );
        }
    }
    Ok(())
}

pub(super) fn show(args: PluginShowArgs) -> Result<()> {
    let path = runtime::package::destination(&plugins_dir()?, &args.name)?;
    let manifest = runtime::package::validate_package(&path)?;
    let activation = Activation::load(Some(&state_path()?))?;
    let row = serde_json::json!({"manifest":manifest,"enabled":!activation.disabled.contains(&args.name),"path":path});
    println!(
        "{}",
        if args.json {
            serde_json::to_string(&row)?
        } else {
            serde_json::to_string_pretty(&row)?
        }
    );
    Ok(())
}

pub(super) async fn update(args: PluginUpdateArgs) -> Result<()> {
    runtime::contract::validate_id(&args.name)?;
    let root = plugins_dir()?;
    if !runtime::package::destination(&root, &args.name)?.is_dir() {
        bail!("Plugin is not installed");
    }
    if let Some(source) = args.source {
        if args.json {
            return review_only(&inspect_local(&source)?);
        }
        install_local(
            &root,
            &source,
            true,
            None,
            Some(&args.name),
            args.accept_command.as_deref(),
            "update",
        )?;
    } else {
        let (entry, bytes) = download(&args.name, None).await?;
        let source = tempfile::NamedTempFile::new()?;
        std::fs::write(source.path(), bytes)?;
        if args.json {
            return review_only(&inspect_archive(source.path())?);
        }
        install_local(
            &root,
            source.path(),
            true,
            Some(&entry.version),
            Some(&args.name),
            args.accept_command.as_deref(),
            "update",
        )?;
    }
    println!(
        "Updated {}; changed native packages require renewed explicit trust",
        args.name
    );
    Ok(())
}

pub(super) fn publish(args: PluginPublishArgs) -> Result<()> {
    if !args.dry_run {
        bail!(
            "Registry publishing is not implemented; only local package preparation is supported"
        );
    }
    let root = package_path(args.path)?.canonicalize()?;
    let manifest = validate::validate(&root)?;
    let files = runtime::package::package_files(&root)?;
    let output = args.output.unwrap_or(PathBuf::from(format!(
        "{}-{}.tar.gz",
        manifest.plugin.id, manifest.plugin.version
    )));
    let output = if output.is_absolute() {
        output
    } else {
        std::env::current_dir()?.join(output)
    };
    if output.starts_with(&root) {
        bail!("Write the archive outside the source package");
    }
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut archive = tar::Builder::new(encoder);
    for relative in files {
        archive.append_path_with_name(
            root.join(&relative),
            Path::new(&manifest.plugin.id).join(relative),
        )?;
    }
    archive.into_inner()?.finish()?.flush()?;
    println!("Prepared {} (not published)", output.display());
    Ok(())
}

async fn bounded_get(url: &str) -> Result<Vec<u8>> {
    let url = reqwest::Url::parse(url)?;
    if url.scheme() != "https"
        || url.host_str() != Some("software.cortex.foundation")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
    {
        bail!("Plugin downloads must use https://software.cortex.foundation");
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("The coding service is temporarily unavailable"))?;
    if !response.status().is_success() {
        bail!("The coding service is temporarily unavailable");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow::anyhow!("The coding service is temporarily unavailable"))?
    {
        if bytes.len() + chunk.len() > runtime::contract::MAX_PACKAGE_BYTES as usize {
            bail!("Download exceeds 64 MiB");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn index() -> Result<runtime::PluginIndex> {
    Ok(serde_json::from_slice(
        &bounded_get(&format!("{REGISTRY}/api/v1/plugins/index")).await?,
    )?)
}

pub(super) async fn search(query: Option<String>, json: bool) -> Result<()> {
    let mut index = index().await?;
    if let Some(query) = query {
        index
            .plugins
            .retain(|p| p.id.contains(&query) || p.description.contains(&query));
    }
    if json {
        println!("{}", serde_json::to_string(&index)?);
    } else {
        for entry in index.plugins {
            println!("{} {} — {}", entry.id, entry.version, entry.description);
        }
    }
    Ok(())
}

async fn download(id: &str, version: Option<&str>) -> Result<(runtime::PluginIndexEntry, Vec<u8>)> {
    let entry = index()
        .await?
        .plugins
        .into_iter()
        .find(|p| p.id == id && version.is_none_or(|v| p.version == v))
        .context("Requested plugin version was not found in the registry")?;
    // Signature policy must be explicit, not silently ignored.
    if entry.signature.is_some() {
        bail!(
            "Signed registry packages require a configured verification key; use a verified local package"
        );
    }
    if entry.checksum.len() != 64 || !entry.checksum.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("Registry package is missing a valid SHA-256 integrity value");
    }
    let bytes = bounded_get(&entry.download_url).await?;
    let temporary = tempfile::NamedTempFile::new()?;
    std::fs::write(temporary.path(), &bytes)?;
    if runtime::package::artifact_hash(temporary.path())? != entry.checksum.to_ascii_lowercase() {
        bail!("Plugin package integrity verification failed");
    }
    Ok((entry, bytes))
}

#[cfg(test)]
#[path = "install_tests.rs"]
mod tests;
