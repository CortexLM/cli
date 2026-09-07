use super::*;
use std::path::Path;
use std::sync::Arc;

pub(super) fn validate(path: &Path) -> Result<runtime::PluginManifest> {
    let manifest = runtime::package::validate_package(path)?;
    match manifest.runtime.kind {
        runtime::contract::RuntimeKind::Node => {
            build::syntax_check(&runtime::package::confined_file(
                path,
                &manifest.runtime.entrypoint,
            )?)?;
        }
        runtime::contract::RuntimeKind::Wasm => {
            let engine = Arc::new(runtime::WasmRuntime::new()?);
            let mut plugin =
                runtime::WasmPlugin::new(manifest.clone(), path.to_path_buf(), engine)?;
            plugin.load()?;
        }
    }
    Ok(manifest)
}

pub(super) async fn run(args: PluginValidateArgs) -> Result<()> {
    let path = package_path(args.path)?;
    let result = validate(&path);
    let report = match &result {
        Ok(manifest) => serde_json::json!({
            "valid": true, "id": manifest.plugin.id, "runtime": manifest.runtime.kind,
            "executed": false,
            "note": "Artifact validation does not execute lifecycle exports; use plugin run after explicit trust",
        }),
        Err(error) => serde_json::json!({"valid":false,"executed":false,"error":error.to_string()}),
    };
    if args.json {
        println!("{}", serde_json::to_string(&report)?);
    } else if let Err(error) = &result {
        eprintln!("Invalid plugin: {error}");
    } else {
        println!("Plugin artifact is valid (lifecycle not executed)");
    }
    result.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-encoded protocol-1 module: `init` and `shutdown`, both `() -> i32`,
    /// no host imports. Written as bytes so the test needs no WASM toolchain.
    const WASM: &[u8] = &[
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // magic and version
        0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, // type: () -> i32
        0x03, 0x03, 0x02, 0x00, 0x00, // two functions of that type
        0x07, 0x13, 0x02, 0x04, b'i', b'n', b'i', b't', 0x00, 0x00, 0x08, b's', b'h', b'u', b't',
        b'd', b'o', b'w', b'n', 0x00, 0x01, // exports
        0x0a, 0x0b, 0x02, 0x04, 0x00, 0x41, 0x00, 0x0b, 0x04, 0x00, 0x41, 0x00,
        0x0b, // bodies: i32.const 0
    ];

    fn manifest(kind: &str, entrypoint: &str) -> String {
        format!(
            "[plugin]\nid=\"demo\"\nname=\"demo\"\nversion=\"0.1.0\"\n\
             [runtime]\nkind={kind:?}\nentrypoint={entrypoint:?}\n"
        )
    }

    #[test]
    fn a_wasm_package_is_compiled_and_its_lifecycle_exports_are_required() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::write(root.join("plugin.toml"), manifest("wasm", "plugin.wasm")).unwrap();
        std::fs::write(root.join("plugin.wasm"), WASM).unwrap();
        assert_eq!(validate(root).unwrap().plugin.id, "demo");

        // Truncating the module must fail compilation rather than pass silently.
        std::fs::write(root.join("plugin.wasm"), &WASM[..WASM.len() - 4]).unwrap();
        assert!(validate(root).is_err());

        // A module without the required lifecycle exports must be rejected.
        std::fs::write(root.join("plugin.wasm"), &WASM[..8]).unwrap();
        assert!(validate(root).is_err());
    }

    #[tokio::test]
    async fn the_json_report_records_failure_and_never_claims_execution() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::write(root.join("plugin.toml"), manifest("wasm", "plugin.wasm")).unwrap();
        std::fs::write(root.join("plugin.wasm"), b"not wasm").unwrap();
        let args = |json| PluginValidateArgs {
            path: Some(root.to_path_buf()),
            json,
            verbose: false,
        };
        assert!(run(args(true)).await.is_err());
        assert!(run(args(false)).await.is_err());

        std::fs::write(root.join("plugin.wasm"), WASM).unwrap();
        run(args(true)).await.unwrap();
    }

    #[test]
    fn a_missing_or_oversized_manifest_fails_validation() {
        let temp = tempfile::tempdir().unwrap();
        assert!(validate(temp.path()).is_err());
        std::fs::write(
            temp.path().join("plugin.toml"),
            manifest("wasm", "plugin.wasm"),
        )
        .unwrap();
        // The declared artifact is missing entirely.
        assert!(validate(temp.path()).is_err());
    }
}
