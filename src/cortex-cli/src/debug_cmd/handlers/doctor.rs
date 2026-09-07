//! Local readiness checks. These do not claim the coding service is reachable.

use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use serde_json::{Value, json};
use tokio::process::Command;

use crate::debug_cmd::DoctorArgs;

fn storage_works(home: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(home)?;
    let mut file = tempfile::NamedTempFile::new_in(home)?;
    file.write_all(b"cortex-local-readiness")?;
    file.flush()?;
    let mut content = String::new();
    file.reopen()?.read_to_string(&mut content)?;
    if content != "cortex-local-readiness" {
        return Err(std::io::Error::other("Storage round-trip failed"));
    }
    file.close()
}

fn config_valid(home: &Path) -> bool {
    let config = home.join("config.toml");
    match std::fs::read_to_string(config) {
        Ok(text) => toml::from_str::<toml::Value>(&text).is_ok(),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

async fn tool_works(name: &str) -> bool {
    let mut command = Command::new(name);
    command.arg("--version").kill_on_drop(true);
    matches!(
        tokio::time::timeout(Duration::from_secs(5), command.output()).await,
        Ok(Ok(output)) if output.status.success()
    )
}

pub(crate) async fn checks(home: &Path) -> Value {
    let storage = storage_works(home).is_ok();
    let configuration = config_valid(home);
    let git = tool_works("git").await;
    let ripgrep = tool_works("rg").await;
    json!({
        "ready": storage && configuration && git && ripgrep,
        "scope": "local",
        "coding_service": "not_checked",
        "checks": {
            "storage": storage,
            "configuration": configuration,
            "git": git,
            "ripgrep": ripgrep,
        },
    })
}

pub async fn run_doctor(args: DoctorArgs) -> Result<()> {
    let home = cortex_common::get_cortex_home()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine the Cortex configuration directory"))?;
    let result = checks(&home).await;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!(
            "Local readiness: {}",
            if result["ready"] == true {
                "ready"
            } else {
                "failed"
            }
        );
        for (name, passed) in result["checks"].as_object().expect("Check object") {
            println!("  {name}: {}", if passed == true { "pass" } else { "fail" });
        }
        println!("Coding service: not checked (no network access)");
    }
    anyhow::ensure!(result["ready"] == true, "Local readiness checks failed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_round_trip_leaves_no_probe_files() {
        let home = tempfile::tempdir().unwrap();
        storage_works(home.path()).unwrap();
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
        assert!(storage_works(&home.path().join("missing/child")).is_ok());
    }

    #[test]
    fn test_invalid_configuration_and_storage_fail() {
        let home = tempfile::tempdir().unwrap();
        assert!(config_valid(home.path()));
        std::fs::write(home.path().join("config.toml"), "broken = [").unwrap();
        assert!(!config_valid(home.path()));
        std::fs::write(home.path().join("file"), "").unwrap();
        assert!(storage_works(&home.path().join("file")).is_err());
    }

    #[tokio::test]
    async fn test_missing_tool_fails_instead_of_reporting_success() {
        assert!(!tool_works("cortex-readiness-nonexistent-command").await);
        assert!(tool_works("git").await);
    }
}
