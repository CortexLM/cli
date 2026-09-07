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

pub(super) async fn install(args: PluginInstallArgs) -> Result<()> {
    let root = plugins_dir()?;
    let local = Path::new(&args.name);
    let id = if local.exists() {
        install_local(&root, local, args.force, args.version.as_deref(), None)?
    } else {
        runtime::contract::validate_id(&args.name)?;
        let (entry, bytes) = download(&args.name, args.version.as_deref()).await?;
        let source = tempfile::NamedTempFile::new()?;
        std::fs::write(source.path(), bytes)?;
        install_local(
            &root,
            source.path(),
            args.force,
            Some(&entry.version),
            Some(&entry.id),
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

pub(super) fn install_local(
    root: &Path,
    source: &Path,
    force: bool,
    version: Option<&str>,
    expected_id: Option<&str>,
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
        install_local(&root, &source, true, None, Some(&args.name))?;
    } else {
        let (entry, bytes) = download(&args.name, None).await?;
        let source = tempfile::NamedTempFile::new()?;
        std::fs::write(source.path(), bytes)?;
        install_local(
            &root,
            source.path(),
            true,
            Some(&entry.version),
            Some(&args.name),
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
mod tests {
    use super::*;
    fn fixture(root: &Path, id: &str) {
        std::fs::create_dir_all(root).unwrap();
        std::fs::write(root.join("plugin.toml"), format!(
            "[plugin]\nid={id:?}\nname={id:?}\nversion=\"0.1.0\"\n[runtime]\nkind=\"node\"\nentrypoint=\"plugin.mjs\"\n")).unwrap();
        std::fs::write(root.join("plugin.mjs"), "export default {};").unwrap();
    }
    #[test]
    fn failed_update_retains_previous_package() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let installs = temp.path().join("installed");
        fixture(&source, "safe");
        install_local(&installs, &source, false, None, None).unwrap();
        let previous = runtime::package::fingerprint(&installs.join("safe")).unwrap();
        std::fs::write(source.join("plugin.mjs"), "invalid javascript !").unwrap();
        assert!(install_local(&installs, &source, true, None, Some("safe")).is_err());
        assert_eq!(
            previous,
            runtime::package::fingerprint(&installs.join("safe")).unwrap()
        );
        assert!(runtime::package::destination(&installs, "../outside").is_err());
    }
    #[test]
    fn symlinks_and_archive_links_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            fixture(temp.path(), "safe");
            std::os::unix::fs::symlink("/etc/passwd", temp.path().join("escape")).unwrap();
            assert!(runtime::package::package_files(temp.path()).is_err());
        }
        let tarball = temp.path().join("link.tar.gz");
        let encoder = flate2::write::GzEncoder::new(
            std::fs::File::create(&tarball).unwrap(),
            flate2::Compression::default(),
        );
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        builder
            .append_link(&mut header, "escape", "../outside")
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap();
        let stage = tempfile::tempdir().unwrap();
        assert!(extract(&tarball, stage.path()).is_err());
        assert!(!stage.path().join("escape").exists());
    }

    /// tar-rs refuses to build traversing paths, so the raw name field is written
    /// directly to reproduce what a hostile archive actually contains.
    fn hostile_archive(path: &Path, names: &[&str]) {
        let encoder = flate2::write::GzEncoder::new(
            std::fs::File::create(path).unwrap(),
            flate2::Compression::default(),
        );
        let mut builder = tar::Builder::new(encoder);
        for name in names {
            let mut header = tar::Header::new_ustar();
            header.set_size(1);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            let field = &mut header.as_old_mut().name;
            field.fill(0);
            field[..name.len()].copy_from_slice(name.as_bytes());
            header.set_cksum();
            builder.append(&header, &b"x"[..]).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn hostile_archive_paths_are_rejected_before_anything_is_written() {
        let temp = tempfile::tempdir().unwrap();
        let deep = (0..17).map(|_| "d").collect::<Vec<_>>().join("/");
        let cases: Vec<Vec<&str>> = vec![
            vec!["../escape"],
            vec!["/etc/passwd"],
            vec!["a/../../escape"],
            vec!["windows\\escape"],
            vec!["same", "same"],
            vec![deep.as_str()],
        ];
        for names in &cases {
            let tarball = temp.path().join("hostile.tar.gz");
            let _ = std::fs::remove_file(&tarball);
            hostile_archive(&tarball, names);
            let stage = tempfile::tempdir().unwrap();
            let error = extract(&tarball, stage.path()).unwrap_err().to_string();
            assert!(error.contains("invalid or duplicate path"), "{names:?}");
            assert!(!temp.path().join("escape").exists());
            assert!(!stage.path().join("d").exists(), "{names:?}");
        }
        // A confined relative path in the same archive shape must still extract.
        let tarball = temp.path().join("ok.tar.gz");
        hostile_archive(&tarball, &["nested/file"]);
        let stage = tempfile::tempdir().unwrap();
        extract(&tarball, stage.path()).unwrap();
        assert_eq!(
            std::fs::read(stage.path().join("nested/file")).unwrap(),
            b"x"
        );
    }

    #[test]
    fn archive_entry_count_and_declared_size_limits_are_enforced() {
        let temp = tempfile::tempdir().unwrap();
        let names: Vec<String> = (0..=runtime::contract::MAX_PACKAGE_FILES)
            .map(|index| format!("f{index}"))
            .collect();
        let tarball = temp.path().join("many.tar.gz");
        hostile_archive(
            &tarball,
            &names.iter().map(String::as_str).collect::<Vec<_>>(),
        );
        let stage = tempfile::tempdir().unwrap();
        assert!(
            extract(&tarball, stage.path())
                .unwrap_err()
                .to_string()
                .contains("1024 entries")
        );

        // A header that lies about a huge size must be rejected on the declared
        // size, before that many bytes are ever written to disk.
        let oversized = temp.path().join("huge.tar.gz");
        let encoder = flate2::write::GzEncoder::new(
            std::fs::File::create(&oversized).unwrap(),
            flate2::Compression::default(),
        );
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_ustar();
        header.set_path("big").unwrap();
        header.set_size(runtime::contract::MAX_PACKAGE_BYTES + 1);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        builder.append(&header, std::io::empty()).unwrap();
        builder.into_inner().unwrap().finish().unwrap();
        let stage = tempfile::tempdir().unwrap();
        assert!(
            extract(&oversized, stage.path())
                .unwrap_err()
                .to_string()
                .contains("exceeds 64 MiB")
        );
    }

    #[test]
    fn archive_root_must_hold_exactly_one_directory_named_after_the_plugin() {
        let temp = tempfile::tempdir().unwrap();
        let flat = temp.path().join("flat");
        fixture(&flat, "safe");
        assert_eq!(package_root(&flat).unwrap(), flat);

        let mismatch = temp.path().join("mismatch");
        fixture(&mismatch.join("other"), "safe");
        assert!(
            package_root(&mismatch)
                .unwrap_err()
                .to_string()
                .contains("must match the plugin ID")
        );

        let nested = temp.path().join("nested");
        fixture(&nested.join("safe"), "safe");
        assert_eq!(package_root(&nested).unwrap(), nested.join("safe"));

        let two = temp.path().join("two");
        fixture(&two.join("safe"), "safe");
        fixture(&two.join("other"), "other");
        assert!(
            package_root(&two)
                .unwrap_err()
                .to_string()
                .contains("one plugin at root")
        );
    }

    #[test]
    fn install_refuses_to_overwrite_or_accept_a_mismatched_identity() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let installs = temp.path().join("installed");
        fixture(&source, "safe");

        assert!(
            install_local(&installs, &source, false, Some("9.9.9"), None)
                .unwrap_err()
                .to_string()
                .contains("identity or version")
        );
        assert!(install_local(&installs, &source, false, None, Some("other")).is_err());
        assert!(!installs.join("safe").exists());

        // Nested package files must be recreated under the staged package.
        std::fs::create_dir(source.join("lib")).unwrap();
        std::fs::write(source.join("lib/helper.mjs"), "export const x = 1;").unwrap();
        install_local(&installs, &source, false, Some("0.1.0"), Some("safe")).unwrap();
        assert!(installs.join("safe/lib/helper.mjs").is_file());

        assert!(
            install_local(&installs, &source, false, None, None)
                .unwrap_err()
                .to_string()
                .contains("--force")
        );
        install_local(&installs, &source, true, None, None).unwrap();

        // Staging directories must never be left behind in the plugin root.
        let leftovers: Vec<_> = std::fs::read_dir(&installs)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name.to_string_lossy().starts_with(".staging-"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn an_invalid_manifest_or_artifact_never_reaches_the_plugin_root() {
        let temp = tempfile::tempdir().unwrap();
        let installs = temp.path().join("installed");
        let broken = temp.path().join("broken");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join("plugin.toml"), "this is not toml [[[").unwrap();
        assert!(install_local(&installs, &broken, false, None, None).is_err());

        let empty = temp.path().join("empty-artifact");
        fixture(&empty, "safe");
        std::fs::write(empty.join("plugin.mjs"), "").unwrap();
        assert!(
            install_local(&installs, &empty, false, None, None)
                .unwrap_err()
                .to_string()
                .contains("Artifact")
        );
        assert!(!installs.join("safe").exists());
    }

    #[test]
    fn a_failed_replacement_rolls_the_previous_package_back() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("installed");
        fixture(&destination, "safe");
        let previous = runtime::package::fingerprint(&destination).unwrap();
        let stage = temp.path().join("stage");
        std::fs::create_dir(&stage).unwrap();

        // A candidate that does not exist makes the second rename fail after the
        // previous package has already been moved aside.
        assert!(replace(&temp.path().join("missing"), &destination, &stage).is_err());
        assert_eq!(
            previous,
            runtime::package::fingerprint(&destination).unwrap()
        );
        assert!(!stage.join("previous").exists());
    }

    #[test]
    fn directory_entries_and_truncated_archive_files_are_handled() {
        let temp = tempfile::tempdir().unwrap();
        let tarball = temp.path().join("dirs.tar.gz");
        let encoder = flate2::write::GzEncoder::new(
            std::fs::File::create(&tarball).unwrap(),
            flate2::Compression::default(),
        );
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_ustar();
        header.set_path("nested").unwrap();
        header.set_size(0);
        header.set_mode(0o755);
        header.set_entry_type(tar::EntryType::Directory);
        header.set_cksum();
        builder.append(&header, std::io::empty()).unwrap();
        let mut header = tar::Header::new_ustar();
        header.set_path("nested/file").unwrap();
        header.set_size(3);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        builder.append(&header, &b"abc"[..]).unwrap();
        builder.into_inner().unwrap().finish().unwrap();
        let stage = tempfile::tempdir().unwrap();
        extract(&tarball, stage.path()).unwrap();
        assert!(stage.path().join("nested").is_dir());
        assert_eq!(
            std::fs::read(stage.path().join("nested/file")).unwrap(),
            b"abc"
        );

        // An archive whose payload stops early must fail, not install a partial file.
        let full = std::fs::read(&tarball).unwrap();
        let truncated = temp.path().join("truncated.tar.gz");
        std::fs::write(&truncated, &full[..full.len() / 2]).unwrap();
        let stage = tempfile::tempdir().unwrap();
        assert!(extract(&truncated, stage.path()).is_err());
    }

    #[test]
    fn an_oversized_compressed_archive_is_refused_before_it_is_opened() {
        let temp = tempfile::tempdir().unwrap();
        let stage = tempfile::tempdir().unwrap();
        // A sparse file keeps this cheap while still exceeding the on-disk limit.
        let big = temp.path().join("big.tar.gz");
        std::fs::File::create(&big)
            .unwrap()
            .set_len(runtime::contract::MAX_PACKAGE_BYTES + 1)
            .unwrap();
        assert!(
            extract(&big, stage.path())
                .unwrap_err()
                .to_string()
                .contains("Compressed package exceeds 64 MiB")
        );
    }

    #[test]
    fn the_install_lock_serializes_mutation_and_is_released_on_drop() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("plugins");
        let held = InstallLock::acquire(&root).unwrap();
        assert!(root.join(".install.lock").is_file());
        let error = InstallLock::acquire(&root)
            .err()
            .expect("a second lock must not be granted")
            .to_string();
        assert!(
            error.contains("Another plugin operation is running"),
            "{error}"
        );
        drop(held);
        assert!(!root.join(".install.lock").exists());
        InstallLock::acquire(&root).unwrap();
    }

    #[test]
    fn publish_prepares_an_installable_archive_and_refuses_to_overwrite() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("safe");
        fixture(&source, "safe");
        let output = temp.path().join("safe-0.1.0.tar.gz");
        let args = || PluginPublishArgs {
            path: Some(source.clone()),
            dry_run: true,
            output: Some(output.clone()),
        };
        publish(args()).unwrap();
        assert!(output.is_file());
        // create_new keeps a second run from clobbering an existing archive.
        assert!(publish(args()).is_err());

        assert!(
            publish(PluginPublishArgs {
                path: Some(source.clone()),
                dry_run: false,
                output: None,
            })
            .unwrap_err()
            .to_string()
            .contains("not implemented")
        );
        assert!(
            publish(PluginPublishArgs {
                path: Some(source.clone()),
                dry_run: true,
                output: Some(source.join("inside.tar.gz")),
            })
            .unwrap_err()
            .to_string()
            .contains("outside the source package")
        );

        let installs = temp.path().join("installed");
        assert_eq!(
            install_local(&installs, &output, false, Some("0.1.0"), Some("safe")).unwrap(),
            "safe"
        );
        assert!(installs.join("safe/plugin.mjs").is_file());
    }

    #[tokio::test]
    async fn downloads_are_restricted_to_the_registry_origin() {
        for url in [
            "http://software.cortex.foundation/plugins/index",
            "https://attacker.example/plugins/index",
            "https://user@software.cortex.foundation/plugins/index",
            "https://user:secret@software.cortex.foundation/plugins/index",
            "https://software.cortex.foundation:8443/plugins/index",
            "file:///etc/passwd",
        ] {
            let error = bounded_get(url).await.unwrap_err().to_string();
            assert!(
                error.contains("https://software.cortex.foundation"),
                "{url}: {error}"
            );
        }
        assert!(bounded_get("not a url").await.is_err());
    }
}
