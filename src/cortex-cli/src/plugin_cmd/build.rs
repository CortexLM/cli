use super::*;
use anyhow::Context;
use std::path::Path;
use std::process::{Command, Stdio};

pub(super) fn check_node() -> Result<()> {
    let output = Command::new(runtime::node::executable()?)
        .env_clear()
        .arg("--version")
        .output()
        .context("Node 22.13+ in the Node 22 LTS line is required")?;
    let version = String::from_utf8(output.stdout)?;
    let parts: Vec<_> = version.trim().trim_start_matches('v').split('.').collect();
    if !output.status.success()
        || parts.first() != Some(&"22")
        || parts
            .get(1)
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0)
            < 13
    {
        bail!("Node 22.13+ in the Node 22 LTS line is required");
    }
    Ok(())
}

pub(super) fn syntax_check(path: &Path) -> Result<()> {
    check_node()?;
    let status = Command::new(runtime::node::executable()?)
        .env_clear()
        .arg("--check")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        bail!("JavaScript artifact failed syntax validation");
    }
    Ok(())
}

pub(super) fn build(path: &Path, debug: bool, trust_code: bool) -> Result<PathBuf> {
    let root = path.canonicalize()?;
    let manifest = runtime::PluginManifest::from_file(root.join("plugin.toml"))?;
    manifest.validate()?;
    // The build output itself must not traverse a symlink or escape the project.
    let destination = output_path(&root, &manifest.runtime.entrypoint)?;
    let temporary = tempfile::Builder::new()
        .prefix(".plugin-build-")
        .tempdir_in(&root)?;
    let artifact = temporary.path().join(match manifest.runtime.kind {
        runtime::contract::RuntimeKind::Node => "plugin.mjs",
        runtime::contract::RuntimeKind::Wasm => "plugin.wasm",
    });
    match manifest.runtime.kind {
        runtime::contract::RuntimeKind::Node => {
            check_node()?;
            let source = runtime::package::confined_file(&root, "src/index.ts")?;
            let code = format!(
                "{}\nbuild(process.argv[1], process.argv[2]);",
                runtime::node::BUILD_SOURCE
            );
            let status = Command::new(runtime::node::executable()?)
                .env_clear()
                .args(["--input-type=module", "--eval", &code, "--"])
                .arg(source)
                .arg(&artifact)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
            if !status.success() {
                bail!(
                    "TypeScript build failed; only erasable types in src/index.ts are supported (no type checking)"
                );
            }
            syntax_check(&artifact)?;
        }
        runtime::contract::RuntimeKind::Wasm => {
            if !trust_code {
                bail!(
                    "Rust builds can execute native build scripts and macros; pass --trust-code only for source you trust"
                );
            }
            build_rust(&root, debug, &artifact)?;
        }
    }
    if !artifact.is_file() || artifact.metadata()?.len() == 0 {
        bail!("Build completed without producing an executable artifact");
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(artifact, &destination)?;
    Ok(destination)
}

fn build_rust(root: &Path, debug: bool, artifact: &Path) -> Result<()> {
    let cargo: toml::Value = toml::from_str(&std::fs::read_to_string(root.join("Cargo.toml"))?)?;
    let name = cargo["package"]["name"]
        .as_str()
        .context("Missing Cargo package name")?;
    runtime::contract::validate_id(name)?;
    let mut command = Command::new("cargo");
    command
        .current_dir(root)
        .args(["build", "--offline", "--target", "wasm32-unknown-unknown"]);
    if !debug {
        command.arg("--release");
    }
    let status = command.stdin(Stdio::null()).status()?;
    if !status.success() {
        bail!("WASM build failed; install the wasm32-unknown-unknown target separately");
    }
    let mode = if debug { "debug" } else { "release" };
    let built = root.join(format!(
        "target/wasm32-unknown-unknown/{mode}/{}.wasm",
        name.replace('-', "_")
    ));
    std::fs::copy(built, artifact)
        .context("Compiler did not produce the expected WASM artifact")?;
    Ok(())
}

fn output_path(root: &Path, relative: &str) -> Result<PathBuf> {
    use std::path::Component;
    let path = Path::new(relative);
    if relative.is_empty()
        || relative.contains('\\')
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        bail!("Entrypoint must be a confined relative file");
    }
    let mut output = root.to_path_buf();
    for part in path.components() {
        output.push(part);
        if let Ok(meta) = output.symlink_metadata() {
            if meta.file_type().is_symlink() {
                bail!("Build path contains a symlink");
            }
        }
    }
    Ok(output)
}

pub(super) fn run(args: PluginBuildArgs) -> Result<()> {
    let path = package_path(args.path)?;
    let artifact = build(&path, args.debug, args.trust_code)?;
    if let Some(output) = args.output {
        std::fs::create_dir_all(&output)?;
        let name = artifact.file_name().context("Missing artifact name")?;
        let destination = output.join(name);
        if destination != artifact {
            std::fs::copy(&artifact, destination)?;
        }
    }
    println!("Built {}", artifact.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str =
        include_str!("../../../../examples/plugins/hello-typescript/src/index.ts");

    fn node_package(root: &Path) {
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("plugin.toml"),
            "[plugin]\nid=\"demo\"\nname=\"demo\"\nversion=\"0.1.0\"\n\
             [runtime]\nkind=\"node\"\nentrypoint=\"dist/plugin.mjs\"\n\
             [[commands]]\nname=\"hello\"\ndescription=\"Greet\"\n",
        )
        .unwrap();
        std::fs::write(root.join("src/index.ts"), EXAMPLE).unwrap();
    }

    #[test]
    fn entrypoints_that_escape_the_package_are_refused() {
        let temp = tempfile::tempdir().unwrap();
        for relative in [
            "",
            "../escape.mjs",
            "/abs.mjs",
            "dist\\plugin.mjs",
            "./x.mjs",
        ] {
            assert!(
                output_path(temp.path(), relative).is_err(),
                "accepted {relative:?}"
            );
        }
        assert_eq!(
            output_path(temp.path(), "dist/plugin.mjs").unwrap(),
            temp.path().join("dist/plugin.mjs")
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_build_path_is_refused_so_output_cannot_be_redirected() {
        let temp = tempfile::tempdir().unwrap();
        let elsewhere = temp.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        let root = temp.path().join("package");
        std::fs::create_dir(&root).unwrap();
        std::os::unix::fs::symlink(&elsewhere, root.join("dist")).unwrap();
        let error = output_path(&root, "dist/plugin.mjs")
            .unwrap_err()
            .to_string();
        assert!(error.contains("symlink"), "{error}");
    }

    #[test]
    fn a_rust_plugin_requires_explicit_source_trust_before_cargo_runs() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("plugin.toml"),
            "[plugin]\nid=\"demo\"\nname=\"demo\"\nversion=\"0.1.0\"\n\
             [runtime]\nkind=\"wasm\"\nentrypoint=\"plugin.wasm\"\n",
        )
        .unwrap();
        let error = build(temp.path(), false, false).unwrap_err().to_string();
        assert!(error.contains("--trust-code"), "{error}");
        assert!(!temp.path().join("plugin.wasm").exists());
    }

    #[test]
    fn an_invalid_manifest_stops_the_build_before_any_toolchain_runs() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("plugin.toml"),
            // A Node runtime cannot declare a .wasm entrypoint.
            "[plugin]\nid=\"demo\"\nname=\"demo\"\nversion=\"0.1.0\"\n\
             [runtime]\nkind=\"node\"\nentrypoint=\"plugin.wasm\"\n",
        )
        .unwrap();
        assert!(build(temp.path(), false, false).is_err());
        assert!(build(temp.path().join("missing").as_path(), false, false).is_err());
    }

    #[test]
    fn typescript_is_stripped_into_a_syntax_checked_artifact() {
        if check_node().is_err() {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        node_package(temp.path());
        let artifact = build(temp.path(), true, false).unwrap();
        assert_eq!(artifact, temp.path().join("dist/plugin.mjs"));
        let code = std::fs::read_to_string(&artifact).unwrap();
        assert!(code.contains("export default"));
        // Type annotations are erased, not compiled away into something new.
        assert!(!code.contains("type Context ="), "{code}");

        // A staged build must not leave its temporary directory behind.
        let leftovers: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name.to_string_lossy().starts_with(".plugin-build-"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");

        let copied = tempfile::tempdir().unwrap();
        run(PluginBuildArgs {
            trust_code: false,
            path: Some(temp.path().to_path_buf()),
            debug: false,
            output: Some(copied.path().join("out")),
        })
        .unwrap();
        assert!(copied.path().join("out/plugin.mjs").is_file());
    }

    #[test]
    fn broken_typescript_fails_the_build_instead_of_emitting_an_artifact() {
        if check_node().is_err() {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        node_package(temp.path());
        std::fs::write(temp.path().join("src/index.ts"), "export default {").unwrap();
        assert!(build(temp.path(), true, false).is_err());
        assert!(!temp.path().join("dist/plugin.mjs").exists());

        // Stripping an empty source produces no artifact; that must fail loudly.
        std::fs::write(temp.path().join("src/index.ts"), "").unwrap();
        let error = build(temp.path(), true, false).unwrap_err().to_string();
        assert!(
            error.contains("without producing an executable artifact"),
            "{error}"
        );
        assert!(!temp.path().join("dist/plugin.mjs").exists());

        // A missing src/index.ts is a confinement failure, not a silent no-op.
        std::fs::remove_file(temp.path().join("src/index.ts")).unwrap();
        assert!(build(temp.path(), true, false).is_err());
    }

    #[test]
    fn the_supported_node_line_is_enforced_by_probing_the_real_runtime() {
        // check_node reports the true state of this machine; both outcomes are
        // real, and neither is stubbed into a green path.
        match check_node() {
            Ok(()) => {
                let version = Command::new(runtime::node::executable().unwrap())
                    .env_clear()
                    .arg("--version")
                    .output()
                    .unwrap();
                let text = String::from_utf8(version.stdout).unwrap();
                assert!(text.trim_start_matches('v').starts_with("22."), "{text}");
            }
            Err(error) => assert!(error.to_string().contains("Node 22.13+")),
        }
    }

    #[test]
    fn syntax_check_rejects_a_javascript_artifact_that_cannot_parse() {
        if check_node().is_err() {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let good = temp.path().join("good.mjs");
        std::fs::write(&good, "export default {};").unwrap();
        syntax_check(&good).unwrap();
        let bad = temp.path().join("bad.mjs");
        std::fs::write(&bad, "export default {").unwrap();
        assert!(syntax_check(&bad).is_err());
        assert!(syntax_check(&temp.path().join("missing.mjs")).is_err());
    }
}
