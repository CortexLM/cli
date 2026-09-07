//! Upgrade command - check for and install updates.
//!
//! Uses the Cortex Software Distribution API at software.cortex.foundation
//! to check for updates and download new versions.

use anyhow::{Context, Result, bail};
use clap::Parser;
use cortex_engine::create_default_client;
use std::io::{Write, stdout};

use cortex_update::{
    ReleaseChannel, SOFTWARE_URL, UpdateConfig, UpdateInfo, UpdateManager, UpdateOutcome,
};

/// Current CLI version from this binary's Cargo.toml
const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Upgrade CLI.
#[derive(Debug, Parser)]
pub struct UpgradeCli {
    /// Target version to upgrade to (e.g., "1.2.0")
    /// If not specified, upgrades to the latest version.
    #[arg(value_name = "VERSION")]
    pub version: Option<String>,

    /// Only check for updates without installing
    #[arg(long, short = 'c')]
    pub check: bool,

    /// Show changelog for the target version
    #[arg(long)]
    pub changelog: bool,

    /// Force upgrade even if already on the target version
    #[arg(long, short = 'f')]
    pub force: bool,

    /// Skip confirmation prompts
    #[arg(long, short = 'y')]
    pub yes: bool,

    /// Release channel to use (stable, beta, nightly)
    #[arg(long, default_value = "stable")]
    pub channel: String,

    /// Include prerelease versions (shorthand for --channel beta).
    /// When specified, allows installing beta/prerelease versions.
    #[arg(long, conflicts_with = "channel")]
    pub pre: bool,

    /// Use custom software distribution URL
    #[arg(long, hide = true)]
    pub url: Option<String>,
}

impl UpgradeCli {
    /// Run the upgrade command.
    pub async fn run(self) -> Result<()> {
        if let Some(version) = &self.version {
            parse_version(version)?;
        }
        println!("Cortex CLI Upgrade");
        println!("{}", "=".repeat(40));
        println!("Current version: v{}", CLI_VERSION);
        println!(
            "Update server: {}",
            self.url.as_deref().unwrap_or(SOFTWARE_URL)
        );

        // Parse channel (--pre is shorthand for --channel beta)
        let channel = if self.pre {
            ReleaseChannel::Beta
        } else {
            match self.channel.as_str() {
                "stable" => ReleaseChannel::Stable,
                "beta" => ReleaseChannel::Beta,
                "nightly" => ReleaseChannel::Nightly,
                _ => {
                    bail!(
                        "Invalid channel: {}. Use: stable, beta, or nightly",
                        self.channel
                    );
                }
            }
        };

        // Create config
        let mut config = UpdateConfig::load();
        config.channel = channel;
        if let Some(url) = &self.url {
            config.custom_url = Some(url.clone());
        }

        // Create update manager
        let manager =
            UpdateManager::with_config(config).context("Failed to initialize update manager")?;

        // Check for specific version or latest
        let update_info = if let Some(ref version) = self.version {
            println!("\nChecking version {}...", version);
            match check_specific_version(&manager, version).await {
                Ok(info) => Some(info),
                Err(_) => bail!(
                    "Could not retrieve the requested Cortex release. No update was installed."
                ),
            }
        } else {
            println!("\nChecking for updates ({} channel)...", self.channel);
            match manager.check_update_forced().await {
                Ok(Some(info)) => {
                    // Show version comparison (Issue #1965)
                    println!(
                        "  Current: v{}  |  Latest: v{}",
                        info.current_version, info.latest_version
                    );
                    Some(info)
                }
                Ok(None) => {
                    println!(
                        "\n✓ You are already on the latest version (v{})",
                        CLI_VERSION
                    );
                    return Ok(());
                }
                Err(_) => {
                    bail!("The update service is temporarily unavailable. No update was installed.")
                }
            }
        };

        let Some(info) = update_info else {
            return Ok(());
        };

        // Display update info
        // Check if versions are the same (or if current is already newer for downgrades)
        let version_cmp = semver_compare(&info.current_version, &info.latest_version)?;

        if version_cmp == 0 && !self.force {
            println!(
                "\n✓ Already on v{} ({} channel). No upgrade needed.",
                info.latest_version, self.channel
            );
            println!("  Use --force to reinstall the same version.");
            return Ok(());
        }

        let is_upgrade = version_cmp < 0;
        if is_upgrade {
            println!(
                "\n→ Update available: v{} → v{} ({} channel)",
                info.current_version, info.latest_version, self.channel
            );
        } else if version_cmp == 0 {
            // Force reinstall case
            println!(
                "\n⟳ Reinstalling v{} ({} channel) (--force)",
                info.latest_version, self.channel
            );
        } else {
            println!(
                "\n↓ Downgrade requested: v{} → v{} ({} channel)",
                info.current_version, info.latest_version, self.channel
            );
        }

        // Show release notes if available
        if let Some(notes) = &info.release_notes {
            println!("\nRelease notes: {}", notes);
        }

        // Show changelog if requested
        if self.changelog {
            if let Some(url) = &info.changelog_url {
                match fetch_and_display_changelog(url).await {
                    Ok(()) => {}
                    Err(e) => {
                        eprintln!("Failed to fetch changelog: {}", e);
                        println!("\nChangelog URL: {}", url);
                    }
                }
            } else {
                println!("\nNo changelog available for this version.");
            }
        }

        // If check-only mode, stop here
        if self.check {
            println!("\nRun `cortex upgrade` to install this version.");
            return Ok(());
        }

        // Confirm before proceeding
        if !self.yes && !self.force {
            print!("\nProceed with upgrade? [y/N] ");
            stdout().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Upgrade cancelled.");
                return Ok(());
            }
        }

        // Perform the upgrade
        perform_upgrade(&manager, &info).await
    }
}

/// Check for a specific version
async fn check_specific_version(manager: &UpdateManager, version: &str) -> Result<UpdateInfo> {
    let version = parse_version(version)?.to_string();
    let info = manager
        .check_version(&version)
        .await
        .map_err(|_| anyhow::anyhow!("Could not retrieve the requested Cortex release"))?;
    if info.latest_version != version {
        bail!("The update service returned a different release. No update was installed.");
    }
    Ok(info)
}

/// Perform the actual upgrade
async fn perform_upgrade(manager: &UpdateManager, info: &UpdateInfo) -> Result<()> {
    // The shared selector does not yet distinguish Linux libc. Never replace a
    // portable musl executable with its GNU sibling while that is unresolved.
    if cfg!(all(target_os = "linux", target_env = "musl")) {
        bail!("Use the verified Cortex installer to update musl installations.");
    }
    if info.install_method.uses_package_manager() {
        bail!(
            "Update this installation through its package manager: brew upgrade cortex, or winget upgrade --id CortexLM.Cortex --exact. No binary was replaced."
        );
    }
    println!("\nDownloading v{}...", info.latest_version);
    println!("  Size: {} bytes", info.asset.size);

    // Download with progress
    let download = manager
        .download_update(info, |progress| {
            let pct = (progress.downloaded as f64 / progress.total as f64 * 100.0) as u32;
            print!(
                "\r  Downloading... {}% ({}/{})",
                pct, progress.downloaded, progress.total
            );
            let _ = stdout().flush();
        })
        .await
        .context("Download failed")?;

    println!("\n  Downloaded successfully");

    // Verify checksum
    print!("Verifying checksum... ");
    stdout().flush()?;
    let mut download = download;
    manager
        .verify(&mut download, &info.asset.sha256)
        .await
        .context("Checksum verification failed")?;
    println!("✓");

    // Install
    print!("Installing... ");
    stdout().flush()?;
    let executable = std::env::current_exe().context("Could not locate the installed binary")?;
    let backup = backup_binary(&executable)?;
    let outcome = manager.install(&download).await.with_context(|| {
        format!(
            "Installation failed. Previous binary retained at {}",
            backup.display()
        )
    })?;
    if let Err(error) = verify_installed_version(&executable, &info.latest_version).await {
        restore_binary(&backup, &executable).with_context(|| {
            format!(
                "Version check failed. Restore {} manually",
                backup.display()
            )
        })?;
        return Err(error.context("Upgrade failed; the previous binary was restored"));
    }
    println!("✓");

    match outcome {
        UpdateOutcome::Updated { from, to } => {
            println!("\n✓ Successfully upgraded from v{} to v{}!", from, to);
            println!(
                "  Verified installed version. Previous binary: {}",
                backup.display()
            );
        }
        UpdateOutcome::RequiresRestart => {
            println!("\n✓ Update installed. Please restart Cortex to complete.");
        }
        _ => {}
    }

    Ok(())
}

/// Stage the recovery copy in the installation directory before replacement.
fn backup_binary(executable: &std::path::Path) -> Result<std::path::PathBuf> {
    let metadata = std::fs::symlink_metadata(executable)?;
    if !metadata.file_type().is_file() {
        bail!("Refusing to replace a non-regular Cortex binary");
    }
    let parent = executable
        .parent()
        .context("Missing installation directory")?;
    let backup = executable.with_extension("old");
    if let Ok(metadata) = std::fs::symlink_metadata(&backup)
        && !metadata.file_type().is_file()
    {
        bail!("Refusing to overwrite a non-regular recovery path");
    }
    let staged = tempfile::NamedTempFile::new_in(parent)?;
    std::fs::copy(executable, staged.path())?;
    staged.as_file().sync_all()?;
    staged.persist(&backup).map_err(|error| error.error)?;
    Ok(backup)
}

fn restore_binary(backup: &std::path::Path, executable: &std::path::Path) -> Result<()> {
    let parent = executable
        .parent()
        .context("Missing installation directory")?;
    let staged = tempfile::NamedTempFile::new_in(parent)?;
    std::fs::copy(backup, staged.path())?;
    staged.as_file().sync_all()?;
    staged.persist(executable).map_err(|error| error.error)?;
    Ok(())
}

async fn verify_installed_version(executable: &std::path::Path, version: &str) -> Result<()> {
    use tokio::io::AsyncReadExt;
    let mut process = tokio::process::Command::new(executable)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("Could not verify the installed Cortex version")?;
    let output = process.stdout.take().context("Missing version output")?;
    let (status, bytes) = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        let mut bytes = Vec::new();
        output.take(4097).read_to_end(&mut bytes).await?;
        if bytes.len() > 4096 {
            bail!("Invalid Cortex version output");
        }
        let status = process.wait().await?;
        Ok::<_, anyhow::Error>((status, bytes))
    })
    .await
    .context("Installed Cortex version check timed out")??;
    let output = std::str::from_utf8(&bytes).context("Invalid Cortex version output")?;
    if !status.success() || output.split_whitespace().last() != Some(version) {
        bail!("Installed Cortex version did not match the requested release");
    }
    Ok(())
}

/// Parse the complete version instead of silently dropping invalid components.
fn parse_version(version: &str) -> Result<semver::Version> {
    semver::Version::parse(version.strip_prefix('v').unwrap_or(version))
        .map_err(|_| anyhow::anyhow!("Invalid Cortex release version"))
}

/// Compare SemVer precedence, including prereleases but not build metadata.
fn semver_compare(a: &str, b: &str) -> Result<i32> {
    Ok(match parse_version(a)?.cmp_precedence(&parse_version(b)?) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    })
}

/// Print a line, handling broken pipe gracefully (Issue #1966).
/// Returns Ok(true) if printed successfully, Ok(false) if pipe was closed.
fn print_line(line: &str) -> Result<bool> {
    use std::io::ErrorKind;
    match writeln!(stdout(), "{}", line) {
        Ok(()) => {
            // Also handle flush errors for piped output
            match stdout().flush() {
                Ok(()) => Ok(true),
                Err(e) if e.kind() == ErrorKind::BrokenPipe => Ok(false),
                Err(e) => Err(e.into()),
            }
        }
        Err(e) if e.kind() == ErrorKind::BrokenPipe => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Convert GitHub URLs to raw content URLs (Issue #3651)
fn convert_to_raw_url(url: &str) -> String {
    // Handle github.com/user/repo/blob/branch/file -> raw.githubusercontent.com/user/repo/branch/file
    if url.contains("github.com") && url.contains("/blob/") {
        return url
            .replace("github.com", "raw.githubusercontent.com")
            .replace("/blob/", "/");
    }

    // Handle github.com/user/repo/releases for changelog
    if url.contains("github.com") && url.contains("/releases") {
        // Try to construct a raw CHANGELOG.md URL
        if let Some(repo_part) = url.split("/releases").next() {
            return format!("{}/raw/main/CHANGELOG.md", repo_part)
                .replace("github.com", "raw.githubusercontent.com")
                .replace("/raw/", "/");
        }
    }

    // Return original URL if no conversion needed
    url.to_string()
}

/// Fetch and display changelog content from a URL
async fn fetch_and_display_changelog(url: &str) -> Result<()> {
    // Convert GitHub URLs to raw content URLs (Issue #3651)
    let raw_url = convert_to_raw_url(url);

    // Create HTTP client
    let client = create_default_client().context("Failed to create HTTP client")?;

    // Fetch the changelog content
    let response = client
        .get(&raw_url)
        .send()
        .await
        .context("Failed to send HTTP request")?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "HTTP request failed with status: {}",
            response.status()
        ));
    }

    let content = response
        .text()
        .await
        .context("Failed to read response body")?;

    // Display the changelog with broken pipe handling (Issue #1966)
    if !print_line(&format!("\n{}", "=".repeat(80)))? {
        return Ok(()); // Pipe closed, exit gracefully
    }
    if !print_line("CHANGELOG")? {
        return Ok(());
    }
    if !print_line(&"=".repeat(80))? {
        return Ok(());
    }
    if !print_line("")? {
        return Ok(());
    }

    // Check if content still looks like HTML (fallback stripping)
    let display_content = if content.trim_start().starts_with("<!DOCTYPE")
        || content.trim_start().starts_with("<html")
        || content.contains("<head>")
    {
        // Content is HTML, strip tags
        strip_html_tags(&content)
    } else {
        content
    };

    // Display with some basic formatting
    for line in display_content.lines() {
        // Indent bullet points slightly
        let output = if line.trim_start().starts_with('-') || line.trim_start().starts_with('*') {
            format!("  {}", line.trim())
        } else if line.trim_start().starts_with('#') {
            // Headers get some spacing
            if !line.trim().is_empty() {
                format!("\n{}\n", line.trim())
            } else {
                continue;
            }
        } else {
            line.to_string()
        };

        if !print_line(&output)? {
            return Ok(()); // Pipe closed, exit gracefully
        }
    }

    let _ = print_line("");
    let _ = print_line(&"=".repeat(80));

    Ok(())
}

/// Strip HTML tags from content (basic implementation)
fn strip_html_tags(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    let mut in_script_or_style = false;
    let mut tag_buffer = String::new();

    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
                tag_buffer.clear();
            }
            '>' => {
                in_tag = false;
                // Check if we're entering or leaving script/style tags
                let tag_lower = tag_buffer.to_lowercase();
                if tag_lower.starts_with("script") || tag_lower.starts_with("style") {
                    in_script_or_style = true;
                } else if tag_lower.starts_with("/script") || tag_lower.starts_with("/style") {
                    in_script_or_style = false;
                }
            }
            _ => {
                if in_tag {
                    tag_buffer.push(ch);
                } else if !in_script_or_style {
                    result.push(ch);
                }
            }
        }
    }

    // Clean up excessive whitespace
    result
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_compare() {
        for (a, b, expected) in [
            ("1.0.0", "1.0.0", 0),
            ("1.0.0", "1.0.1", -1),
            ("1.0.1", "1.0.0", 1),
            ("v1.0.0", "1.0.0", 0),
            ("1.0.0-beta.2", "1.0.0-beta.10", -1),
            ("1.0.0-beta", "1.0.0", -1),
            ("1.0.0+old", "1.0.0+new", 0),
        ] {
            assert_eq!(semver_compare(a, b).unwrap(), expected);
        }
    }

    #[test]
    fn invalid_versions_are_rejected_before_any_lookup() {
        for version in [
            "1.0",
            "1.0.0.1",
            "vv1.0.0",
            "../1.0.0",
            "1.0.no",
            "1.0.0-beta.01",
        ] {
            assert!(parse_version(version).is_err(), "{version}");
        }
    }

    #[test]
    fn binary_backup_and_restore_preserve_recovery_copy() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("Cortex");
        std::fs::write(&binary, b"previous binary").unwrap();
        let backup = backup_binary(&binary).unwrap();
        std::fs::write(&binary, b"invalid new binary").unwrap();
        restore_binary(&backup, &binary).unwrap();
        assert_eq!(std::fs::read(&binary).unwrap(), b"previous binary");
        assert_eq!(std::fs::read(&backup).unwrap(), b"previous binary");
    }

    #[test]
    fn backup_refuses_to_overwrite_a_non_regular_recovery_path() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("Cortex");
        std::fs::write(&binary, b"previous binary").unwrap();
        // `Cortex.old` is the fixed recovery path; a directory there means the
        // previous binary could not be preserved, so replacement must not start.
        std::fs::create_dir(binary.with_extension("old")).unwrap();
        assert!(backup_binary(&binary).is_err());
        assert_eq!(std::fs::read(&binary).unwrap(), b"previous binary");
    }

    #[test]
    fn backup_and_restore_require_a_real_installation_directory() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing-dir").join("Cortex");
        assert!(backup_binary(&missing).is_err());
        let backup = dir.path().join("source");
        std::fs::write(&backup, b"previous binary").unwrap();
        assert!(restore_binary(&backup, &missing).is_err());
    }

    #[test]
    fn changelog_urls_resolve_to_raw_content_without_leaving_the_host() {
        assert_eq!(
            convert_to_raw_url("https://github.com/owner/repo/blob/main/CHANGELOG.md"),
            "https://raw.githubusercontent.com/owner/repo/main/CHANGELOG.md"
        );
        assert_eq!(
            convert_to_raw_url("https://github.com/owner/repo/releases"),
            "https://raw.githubusercontent.com/owner/repo/main/CHANGELOG.md"
        );
        // Anything else is passed through unchanged rather than rewritten.
        for url in [
            "https://software.cortex.foundation/changelog",
            "https://example.test/notes.md",
        ] {
            assert_eq!(convert_to_raw_url(url), url);
        }
    }

    #[test]
    fn html_changelogs_are_stripped_of_markup_and_scripts() {
        let stripped = strip_html_tags(
            "<html><head><style>body{color:red}</style></head><body>\n\
             <script>steal()</script>\n<h1>Release 1.2.3</h1>\n<p>Fixed a bug</p>\n\
             </body></html>",
        );
        assert_eq!(stripped, "Release 1.2.3\nFixed a bug");
        assert!(!stripped.contains("steal"));
        assert!(!stripped.contains("color:red"));
    }

    #[test]
    fn printing_a_line_succeeds_on_an_open_stream() {
        // The broken-pipe branch needs a closed downstream reader, which the
        // captured harness stdout never is; it stays uncovered deliberately.
        assert!(print_line("changelog line").unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn backup_rejects_symlink_targets() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("other");
        let binary = dir.path().join("Cortex");
        std::fs::write(&target, b"other command").unwrap();
        std::os::unix::fs::symlink(&target, &binary).unwrap();
        assert!(backup_binary(&binary).is_err());
        assert_eq!(std::fs::read(target).unwrap(), b"other command");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn installed_version_must_be_successful_and_exact() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("Cortex");
        for (body, valid) in [
            ("echo Cortex 9.8.7", true),
            ("echo Cortex 9.8.6", false),
            ("echo Cortex 9.8.7; exit 1", false),
            ("head -c 5000 /dev/zero", false),
        ] {
            std::fs::write(&binary, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
            let result = verify_installed_version(&binary, "9.8.7").await;
            assert_eq!(result.is_ok(), valid, "{body}: {result:?}");
        }
    }
}
