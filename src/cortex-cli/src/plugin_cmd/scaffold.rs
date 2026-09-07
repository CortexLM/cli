use super::*;

pub(super) const TYPESCRIPT: &str =
    include_str!("../../../../examples/plugins/hello-typescript/src/index.ts");

pub(super) fn create(args: PluginNewArgs) -> Result<()> {
    runtime::contract::validate_id(&args.name)?;
    if args.advanced {
        bail!("Arbitrary TUI hooks are not supported; use validated notifications");
    }
    let base = args.output.unwrap_or(std::env::current_dir()?);
    std::fs::create_dir_all(&base)?;
    let root = base.join(&args.name);
    if root.exists() {
        bail!("Plugin directory already exists");
    }
    std::fs::create_dir(&root)?;
    std::fs::create_dir(root.join("src"))?;
    let value = serde_json::json!({
        "capabilities":["commands","hooks","tools"],
        "permissions":["notifications"],
        "plugin": {
            "id":args.name,"name":args.name,"version":"0.1.0",
            "description":args.description,"authors":args.author.into_iter().collect::<Vec<_>>(),
            "license":"Apache-2.0",
        },
        "runtime": {
            "kind":if args.typescript { "node" } else { "wasm" },
            "protocol":1,
            "entrypoint":if args.typescript { "dist/plugin.mjs" } else { "plugin.wasm" },
            "timeout_ms":5000,
        },
        "commands":[{"name":"hello","description":"Greet the supplied name"}],
        "hooks": if args.typescript { serde_json::json!([
            {"hook_type":"session_start","function":"session_start"},
            {"hook_type":"tool_execute_before","function":"tool_before"},
            {"hook_type":"session_end","function":"session_end"}
        ]) } else { serde_json::json!([]) },
        "tools": if args.typescript { serde_json::json!([{
            "name":"greet","description":"Greet a name using persistent plugin state",
            "input_schema":{"type":"object","additionalProperties":false,
                "properties":{"name":{"type":"string"}},"required":["name"]}
        }]) } else { serde_json::json!([]) },
    });
    let manifest: runtime::PluginManifest = serde_json::from_value(value)?;
    manifest.validate()?;
    std::fs::write(root.join("plugin.toml"), toml::to_string_pretty(&manifest)?)?;
    if args.typescript {
        std::fs::write(root.join("src/index.ts"), TYPESCRIPT)?;
    } else {
        std::fs::write(root.join("src/lib.rs"), RUST)?;
        std::fs::write(
            root.join("Cargo.toml"),
            format!(
                "[package]\nname = {:?}\nversion = \"0.1.0\"\nedition = \"2021\"\nlicense = \"Apache-2.0\"\n[lib]\ncrate-type = [\"cdylib\"]\n[profile.release]\npanic = \"abort\"\n[workspace]\n",
                args.name
            ),
        )?;
    }
    std::fs::write(
        root.join("README.md"),
        format!(
            "# {}\n\nBuild: `Cortex plugin build --path .`\n\nValidate: `Cortex plugin validate --path .`\n\nInstall only code you trust: `Cortex plugin install . --trust-code`\n\nInvoke: `Cortex plugin run {} hello World`\n\nNode plugins execute native code and are NOT sandboxed. No package scripts or dependencies are installed by Cortex. Type stripping does not type-check.\n",
            args.name, args.name
        ),
    )?;
    println!("Created {}", root.display());
    Ok(())
}

// Guest FFI belongs to the generated no_std module, never to the native host.
const RUST: &str = r##"#![no_std]
#[link(wasm_import_module = "cortex")]
extern "C" { fn set_result(ptr: i32, len: i32) -> i32; }
#[no_mangle]
pub extern "C" fn init() -> i32 { 0 }
#[no_mangle]
pub extern "C" fn shutdown() -> i32 { 0 }
#[no_mangle]
pub extern "C" fn cmd_hello() -> i32 {
    let result = br#"{"data":"Hello from a WASM plugin"}"#;
    unsafe { set_result(result.as_ptr() as i32, result.len() as i32) }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
"##;

#[cfg(test)]
mod tests {
    use super::*;

    fn args(name: &str, base: &std::path::Path, typescript: bool) -> PluginNewArgs {
        PluginNewArgs {
            name: name.into(),
            description: "A Cortex plugin".into(),
            author: Some("Example".into()),
            output: Some(base.to_path_buf()),
            advanced: false,
            typescript,
        }
    }

    #[test]
    fn the_typescript_scaffold_is_a_valid_installable_package_source() {
        let temp = tempfile::tempdir().unwrap();
        create(args("demo-ts", temp.path(), true)).unwrap();
        let root = temp.path().join("demo-ts");
        assert_eq!(
            std::fs::read_to_string(root.join("src/index.ts")).unwrap(),
            TYPESCRIPT
        );
        assert!(!root.join("Cargo.toml").exists());
        let manifest = runtime::PluginManifest::from_file(root.join("plugin.toml")).unwrap();
        manifest.validate().unwrap();
        assert_eq!(manifest.plugin.id, "demo-ts");
        assert_eq!(manifest.runtime.kind, runtime::contract::RuntimeKind::Node);
        assert_eq!(manifest.runtime.entrypoint, "dist/plugin.mjs");
        assert_eq!(manifest.tools.len(), 1);
        assert_eq!(manifest.hooks.len(), 3);
        let readme = std::fs::read_to_string(root.join("README.md")).unwrap();
        assert!(readme.contains("NOT sandboxed"), "{readme}");
    }

    #[test]
    fn the_rust_scaffold_declares_a_wasm_cdylib_without_hooks_or_tools() {
        let temp = tempfile::tempdir().unwrap();
        create(args("demo-rs", temp.path(), false)).unwrap();
        let root = temp.path().join("demo-rs");
        let manifest = runtime::PluginManifest::from_file(root.join("plugin.toml")).unwrap();
        manifest.validate().unwrap();
        assert_eq!(manifest.runtime.kind, runtime::contract::RuntimeKind::Wasm);
        assert_eq!(manifest.runtime.entrypoint, "plugin.wasm");
        assert!(manifest.tools.is_empty() && manifest.hooks.is_empty());
        let cargo: toml::Value =
            toml::from_str(&std::fs::read_to_string(root.join("Cargo.toml")).unwrap()).unwrap();
        assert_eq!(cargo["package"]["name"].as_str(), Some("demo-rs"));
        assert_eq!(cargo["lib"]["crate-type"][0].as_str(), Some("cdylib"));
        let source = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();
        assert!(source.starts_with("#![no_std]"));
        assert!(source.contains("cmd_hello"));
    }

    #[test]
    fn hostile_names_advanced_hooks_and_existing_directories_are_refused() {
        let temp = tempfile::tempdir().unwrap();
        for name in ["../escape", "bad id", "", "a/b", &"x".repeat(65)] {
            assert!(create(args(name, temp.path(), true)).is_err(), "{name:?}");
        }
        assert!(!temp.path().join("escape").exists());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);

        let mut advanced = args("demo-ts", temp.path(), true);
        advanced.advanced = true;
        assert!(
            create(advanced)
                .unwrap_err()
                .to_string()
                .contains("not supported")
        );

        create(args("demo-ts", temp.path(), true)).unwrap();
        assert!(
            create(args("demo-ts", temp.path(), true))
                .unwrap_err()
                .to_string()
                .contains("already exists")
        );
    }
}
