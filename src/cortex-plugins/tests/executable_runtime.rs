//! Real local module execution, not mocked lifecycle success.
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use cortex_plugins::activation::Activation;
use cortex_plugins::contract::{RuntimeKind, validate_arguments, validate_schema};
use cortex_plugins::{
    HookType, Plugin, PluginConfig, PluginContext, PluginManager, PluginManifest, WasmPlugin,
    WasmRuntime,
};
use serde_json::{Value, json};

const EXAMPLE: &str = include_str!("../../../examples/plugins/hello-typescript/src/index.ts");
const MANIFEST: &str = include_str!("../../../examples/plugins/hello-typescript/plugin.toml");
const WAT: &str = include_str!("fixtures/protocol_v1.wat");

fn node_package(root: &Path) {
    std::fs::create_dir_all(root.join("dist")).unwrap();
    std::fs::write(root.join("plugin.toml"), MANIFEST).unwrap();
    let source = root.join("example.ts");
    std::fs::write(&source, EXAMPLE).unwrap();
    let script = format!(
        "{}\nbuild(process.argv[1],process.argv[2]);",
        cortex_plugins::node::BUILD_SOURCE
    );
    let output = std::process::Command::new("node")
        .env_clear()
        .args(["--input-type=module", "--eval", &script, "--"])
        .arg(source)
        .arg(root.join("dist/plugin.mjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn config(root: &Path, state: &Path) -> PluginConfig {
    PluginConfig {
        search_paths: vec![root.into()],
        state_path: Some(state.into()),
        ..Default::default()
    }
}

fn trust(root: &Path, state: &Path) {
    let mut activation = Activation::default();
    activation.trusted.insert(
        "hello-typescript".into(),
        cortex_plugins::package::fingerprint(root).unwrap(),
    );
    activation.save(state).unwrap();
}

#[tokio::test]
async fn node_build_load_lifecycle_tool_mutation_veto_and_durable_disable() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("plugins/hello-typescript");
    let state = temp.path().join("plugins.json");
    node_package(&root);
    let config = config(&temp.path().join("plugins"), &state);
    let untrusted = PluginManager::new(config.clone()).await.unwrap();
    assert!(untrusted.load_from_path(&root).await.is_err());
    trust(&root, &state);
    let manager = PluginManager::new(config.clone()).await.unwrap();
    manager.discover_and_load().await.unwrap();
    manager.init_all().await.unwrap();
    let context = PluginContext::new(temp.path())
        .with_session("session-fixture")
        .with_extra("call_id", json!("call-42"));
    let started = manager
        .dispatch_hook(HookType::SessionStart, json!({}), &context)
        .await
        .unwrap();
    assert_eq!(
        started.notifications[0].message,
        "TypeScript plugin session started"
    );
    let before = manager
        .dispatch_hook(
            HookType::ToolExecuteBefore,
            json!({"tool":"greet","args":{"name":"  Ada  "}}),
            &context,
        )
        .await
        .unwrap();
    assert_eq!(before.input["args"]["name"], "Ada");
    let result = manager
        .execute_tool(
            "hello-typescript",
            "greet",
            before.input["args"].clone(),
            &context,
        )
        .await
        .unwrap();
    assert_eq!(
        result.data,
        json!({"greeting":"Hello, Ada!","calls":1,"call_id":"call-42"})
    );
    let result = manager
        .execute_tool("hello-typescript", "greet", json!({"name":"Lin"}), &context)
        .await
        .unwrap();
    assert_eq!(result.data["calls"], 2);
    assert!(
        manager
            .execute_tool("hello-typescript", "greet", json!({"name":false}), &context)
            .await
            .is_err()
    );
    let veto = manager
        .dispatch_hook(
            HookType::ToolExecuteBefore,
            json!({"tool":"greet","args":{"name":"blocked"}}),
            &context,
        )
        .await
        .unwrap();
    assert!(veto.denied);
    assert_eq!(manager.list_tools().await.len(), 1);
    manager.disable("hello-typescript").await.unwrap();
    assert!(
        manager
            .execute_command("hello", vec![], &context)
            .await
            .is_err()
    );
    let restarted = PluginManager::new(config).await.unwrap();
    assert!(restarted.discover_and_load().await.unwrap().is_empty());
    assert!(restarted.list_tools().await.is_empty());
}

async fn node_with_command(
    root: &Path,
    body: &str,
    timeout: u64,
) -> cortex_plugins::node::NodePlugin {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(root.join("plugin.mjs"), format!(
        "export default {{protocol:1,init:()=>({{data:null}}),shutdown:()=>({{data:null}}),commands:{{test:{body}}}}};"
    )).unwrap();
    let manifest = PluginManifest::parse(&format!(
        "[plugin]\nid=\"fixture\"\nname=\"Fixture\"\nversion=\"0.1.0\"\n[runtime]\nkind=\"node\"\nentrypoint=\"plugin.mjs\"\ntimeout_ms={timeout}\n[[commands]]\nname=\"test\"\ndescription=\"test\"\n")).unwrap();
    let hash = cortex_plugins::package::fingerprint(root).unwrap();
    let mut plugin = cortex_plugins::node::NodePlugin::new(manifest, root.into(), &hash).unwrap();
    plugin.init().await.unwrap();
    plugin
}

#[tokio::test]
async fn node_timeout_crash_malformed_output_and_exception_fail_closed() {
    let cases = [
        "()=>{while(true){}}",
        "()=>{process.exit(9)}",
        "()=>{throw new Error('private exception')}",
        "()=>{process.stdout.write('not-json\\n'); return {data:null}}",
        "()=>({data:'x'.repeat(1024*1024+1)})",
        "()=>({data:null,notifications:[{level:'info',message:'not permitted'}]})",
    ];
    for body in cases {
        let temp = tempfile::tempdir().unwrap();
        let mut plugin = node_with_command(temp.path(), body, 800).await;
        let result = plugin
            .execute_command("test", vec![], &PluginContext::new(temp.path()))
            .await;
        assert!(result.is_err(), "Body should fail: {body}");
        // Explicit shutdown must always reap a failed child.
        let _ = plugin.shutdown().await;
    }
}

#[tokio::test]
async fn cancellation_stops_worker_and_explicit_restart_resets_state() {
    let temp = tempfile::tempdir().unwrap();
    let mut plugin = node_with_command(temp.path(), "()=>new Promise(()=>{})", 5000).await;
    let context = PluginContext::new(temp.path());
    assert!(
        tokio::time::timeout(
            Duration::from_millis(50),
            plugin.execute_command("test", vec![], &context)
        )
        .await
        .is_err()
    );
    assert!(
        plugin
            .execute_command("test", vec![], &context)
            .await
            .is_err()
    );
    assert_eq!(plugin.state(), cortex_plugins::PluginState::Error);
    plugin.init().await.unwrap();
    assert_eq!(plugin.state(), cortex_plugins::PluginState::Active);
    plugin.shutdown().await.unwrap();
}

fn wasm_plugin(root: &Path, wat: &str) -> WasmPlugin {
    std::fs::write(root.join("plugin.wasm"), wat).unwrap();
    let manifest = PluginManifest::parse(
        "[plugin]\nid=\"wasm-fixture\"\nname=\"WASM Fixture\"\nversion=\"0.1.0\"\n[runtime]\ntimeout_ms=100\n\
         [[commands]]\nname=\"echo\"\ndescription=\"echo\"\n\
         [[commands]]\nname=\"count\"\ndescription=\"count\"\n\
         [[commands]]\nname=\"loop\"\ndescription=\"loop\"\n\
         [[commands]]\nname=\"bounds\"\ndescription=\"bounds\"\n\
         [[commands]]\nname=\"memory\"\ndescription=\"memory\"\n"
    ).unwrap();
    assert_eq!(manifest.runtime.kind, RuntimeKind::Wasm);
    WasmPlugin::new(manifest, root.into(), Arc::new(WasmRuntime::new().unwrap())).unwrap()
}

#[tokio::test]
async fn wasm_context_arguments_state_and_lifecycle_are_real() {
    let temp = tempfile::tempdir().unwrap();
    let mut plugin = wasm_plugin(temp.path(), WAT);
    plugin.load().unwrap();
    plugin.init().await.unwrap();
    let context = PluginContext::new(temp.path())
        .with_session("wasm-session")
        .with_extra("call_id", json!("wasm-42"));
    let response = plugin
        .execute_command("echo", vec!["actual-argument".into()], &context)
        .await
        .unwrap();
    let value: Value = serde_json::from_str(&response).unwrap();
    assert_eq!(value["input"], json!(["actual-argument"]));
    assert_eq!(value["context"]["session_id"], "wasm-session");
    assert_eq!(value["context"]["extra"]["call_id"], "wasm-42");
    assert_eq!(
        plugin
            .execute_command("count", vec![], &context)
            .await
            .unwrap(),
        "1"
    );
    assert_eq!(
        plugin
            .execute_command("count", vec![], &context)
            .await
            .unwrap(),
        "2"
    );
    assert_eq!(
        plugin
            .execute_command("memory", vec![], &context)
            .await
            .unwrap(),
        "-1"
    );
    plugin.shutdown().await.unwrap();
    assert!(
        plugin
            .execute_command("count", vec![], &context)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn wasm_errors_bounds_memory_and_fuel_are_not_success() {
    for command in ["loop", "bounds"] {
        let temp = tempfile::tempdir().unwrap();
        let mut plugin = wasm_plugin(temp.path(), WAT);
        plugin.load().unwrap();
        plugin.init().await.unwrap();
        assert_eq!(
            plugin
                .execute_command("count", vec![], &PluginContext::new(temp.path()))
                .await
                .unwrap(),
            "1"
        );
        assert!(
            plugin
                .execute_command(command, vec![], &PluginContext::new(temp.path()))
                .await
                .is_err()
        );
        assert_eq!(plugin.state(), cortex_plugins::PluginState::Error);
        plugin.init().await.unwrap();
        assert_eq!(
            plugin
                .execute_command("count", vec![], &PluginContext::new(temp.path()))
                .await
                .unwrap(),
            "1"
        );
        plugin.shutdown().await.unwrap();
    }
    let temp = tempfile::tempdir().unwrap();
    let wat = WAT.replace(
        "(export \"init\") (result i32) i32.const 0",
        "(export \"init\") (result i32) i32.const 9",
    );
    let mut plugin = wasm_plugin(temp.path(), &wat);
    plugin.load().unwrap();
    assert!(plugin.init().await.is_err());
    let mut plugin = wasm_plugin(temp.path(), "(module)");
    assert!(plugin.load().is_err());
    let wat = WAT.replace(
        "(export \"shutdown\") (result i32) i32.const 0",
        "(export \"shutdown\") (result i32) unreachable",
    );
    let mut plugin = wasm_plugin(temp.path(), &wat);
    plugin.load().unwrap();
    plugin.init().await.unwrap();
    assert!(plugin.shutdown().await.is_err());
}

#[test]
fn schemas_and_permission_monotonicity_reject_unsupported_contracts() {
    assert!(
        serde_json::from_value::<cortex_plugins::contract::HookOutcome>(
            json!({"decision":"allow"})
        )
        .is_err()
    );
    assert!(
        validate_schema(
            &json!({"type":"object","additionalProperties":true,"properties":{}}),
            0
        )
        .is_err()
    );
    assert!(validate_arguments(&json!({"type":"string"}), &json!(false)).is_err());
}

#[tokio::test]
async fn hooks_are_ordered_and_reload_does_not_double_register() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("plugins");
    let state = temp.path().join("activation.json");
    let mut activation = Activation::default();
    for (id, priority) in [("first", 20), ("second", 10)] {
        let directory = root.join(id);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("plugin.toml"), format!(
            "[plugin]\nid={id:?}\nname={id:?}\nversion=\"0.1.0\"\n[runtime]\nkind=\"node\"\nentrypoint=\"plugin.mjs\"\n[[hooks]]\nhook_type=\"session_start\"\nfunction=\"start\"\npriority={priority}\n")).unwrap();
        std::fs::write(directory.join("plugin.mjs"), format!(
            "export default {{protocol:1,init:()=>({{data:null}}),shutdown:()=>({{data:null}}),hooks:{{start:(input)=>({{data:{{decision:'continue',input:{{order:[...input.order,{id:?}]}}}}}})}}}};")).unwrap();
        activation.trusted.insert(
            id.into(),
            cortex_plugins::package::fingerprint(&directory).unwrap(),
        );
    }
    activation.save(&state).unwrap();
    let manager = PluginManager::new(config(&root, &state)).await.unwrap();
    manager.discover_and_load().await.unwrap();
    manager.init_all().await.unwrap();
    let ctx = PluginContext::new(temp.path());
    let result = manager
        .dispatch_hook(HookType::SessionStart, json!({"order":[]}), &ctx)
        .await
        .unwrap();
    assert_eq!(result.input["order"], json!(["second", "first"]));
    manager.unload("first").await.unwrap();
    manager.load_from_path(&root.join("first")).await.unwrap();
    manager.init_all().await.unwrap();
    let result = manager
        .dispatch_hook(HookType::SessionStart, json!({"order":[]}), &ctx)
        .await
        .unwrap();
    assert_eq!(result.input["order"], json!(["second", "first"]));
    manager.shutdown_all().await.unwrap();
}

#[tokio::test]
async fn node_rejects_changed_trust_missing_exports_and_init_failure() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let manifest = PluginManifest::parse("[plugin]\nid=\"fixture\"\nname=\"fixture\"\nversion=\"0.1.0\"\n[runtime]\nkind=\"node\"\nentrypoint=\"plugin.mjs\"\n").unwrap();
    for source in [
        "export default {protocol:1};",
        "export default {protocol:1,init:()=>{throw Error('failure')},shutdown:()=>({data:null})};",
    ] {
        std::fs::write(root.join("plugin.mjs"), source).unwrap();
        let hash = cortex_plugins::package::fingerprint(root).unwrap();
        let mut plugin =
            cortex_plugins::node::NodePlugin::new(manifest.clone(), root.into(), &hash).unwrap();
        assert!(plugin.init().await.is_err());
    }
    let hash = cortex_plugins::package::fingerprint(root).unwrap();
    std::fs::write(root.join("plugin.mjs"), "export default {};").unwrap();
    assert!(cortex_plugins::node::NodePlugin::new(manifest, root.into(), &hash).is_err());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn shutdown_kills_ordinary_descendants_in_the_worker_process_group() {
    let temp = tempfile::tempdir().unwrap();
    let mut plugin = node_with_command(temp.path(),
        "async()=>{const {spawn}=await import('node:child_process'); const child=spawn(process.execPath,['-e','setTimeout(()=>{},60000)'],{stdio:'ignore'}); return {data:child.pid}}", 2000).await;
    let pid = plugin
        .execute_command("test", vec![], &PluginContext::new(temp.path()))
        .await
        .unwrap();
    let path = format!("/proc/{pid}/stat");
    assert!(std::fs::metadata(&path).is_ok());
    plugin.shutdown().await.unwrap();
    for _ in 0..20 {
        match std::fs::read_to_string(&path) {
            Err(_) => return,
            Ok(stat)
                if stat
                    .rsplit_once(')')
                    .is_some_and(|(_, rest)| rest.trim_start().starts_with('Z')) =>
            {
                return;
            }
            _ => tokio::time::sleep(Duration::from_millis(25)).await,
        }
    }
    panic!("Plugin descendant {pid} remained running after shutdown");
}
