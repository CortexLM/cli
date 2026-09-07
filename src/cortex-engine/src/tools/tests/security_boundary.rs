//! Real registry/router boundary tests. Subprocesses use only isolated fixtures.
use crate::tools::{
    PluginTool, ToolContext, ToolDefinition, ToolHandler, ToolRegistry, ToolResult, ToolRouter,
};
use async_trait::async_trait;
use cortex_protocol::SandboxPolicy;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;

async fn call(
    registry: &ToolRegistry,
    name: &str,
    args: Value,
    context: ToolContext,
) -> ToolResult {
    registry
        .execute_with_context(name, args, context)
        .await
        .unwrap_or_else(|e| ToolResult::error(e.to_string()))
}

#[tokio::test]
async fn security_boundary_direct_batch_child_share_denials() {
    let root = tempdir().unwrap();
    let registry = ToolRegistry::new();
    let context = ToolContext::new(root.path().into());
    let args = json!({"file_path": "denied.txt", "content": "not written"});
    let direct = call(&registry, "Create", args.clone(), context.clone()).await;
    let child = call(
        &registry,
        "Create",
        args.clone(),
        context.clone().for_child(),
    )
    .await;
    let batch = call(
        &registry,
        "Batch",
        json!({"calls": [{"tool": "Create", "arguments": args}]}),
        context.clone(),
    )
    .await;
    assert!(!direct.success && !child.success && !batch.success);
    assert_eq!(direct.output, child.output);
    assert!(batch.output.contains(&direct.output));
    let router = ToolRouter::new();
    let routed = router
        .execute(
            "Create",
            json!({"file_path":"denied.txt","content":"not written"}),
            &context,
        )
        .await
        .unwrap();
    assert_eq!(routed.output, direct.output);
    assert!(!root.path().join("denied.txt").exists());
}

#[tokio::test]
async fn security_boundary_explicit_approval_is_exact_once_and_deny_wins() {
    let root = tempdir().unwrap();
    let registry = ToolRegistry::new();
    let args = json!({"file_path":"nested/new.txt","content":"approved"});
    let context = ToolContext::new(root.path().into()).with_approved_tool_call("Create", &args);
    let changed = call(
        &registry,
        "Create",
        json!({"file_path":"other.txt","content":"approved"}),
        context.clone(),
    )
    .await;
    assert!(!changed.success);
    let accepted = call(&registry, "Create", args.clone(), context.clone()).await;
    assert!(accepted.success, "{}", accepted.output);
    assert_eq!(
        std::fs::read_to_string(root.path().join("nested/new.txt")).unwrap(),
        "approved"
    );
    assert!(
        !call(&registry, "Create", args.clone(), context)
            .await
            .success
    );
    let denied = ToolContext::new(root.path().into())
        .with_auto_approve(true)
        .with_denied_tools(vec!["Create".into()]);
    assert!(!call(&registry, "Create", args, denied).await.success);
}

#[tokio::test]
async fn security_boundary_plan_cannot_be_unlocked_by_arguments_or_environment() {
    let root = tempdir().unwrap();
    let registry = ToolRegistry::new();
    let mut context = ToolContext::new(root.path().into())
        .with_read_only(true)
        .with_auto_approve(true);
    context.env.insert("CORTEX_SPEC_MODE".into(), "0".into());
    context
        .env
        .insert("CORTEX_OPERATION_MODE".into(), "code".into());
    for (name, args) in [
        ("Create", json!({"file_path":"no.txt","content":"no"})),
        (
            "Execute",
            json!({"command":["echo","not started"],"auto_approve":true}),
        ),
        ("MultiEdit", json!({"edits":[]})),
    ] {
        let direct = call(&registry, name, args.clone(), context.clone()).await;
        let batch = call(
            &registry,
            "Batch",
            json!({"calls":[{"tool":name,"arguments":args}]}),
            context.clone(),
        )
        .await;
        assert!(!direct.success && !batch.success);
        assert!(direct.output.contains("Read-only"));
        assert!(batch.output.contains("Read-only"));
    }
    let child = context.for_child();
    assert!(
        !call(&registry, "Task", json!({"prompt":"nested"}), child)
            .await
            .success
    );
}

#[tokio::test]
async fn security_boundary_read_pagination_and_result_redaction() {
    let root = tempdir().unwrap();
    std::fs::write(
        root.path().join("page.txt"),
        "first\nsecond\nthird\nfourth\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("secret.txt"),
        "synthetic-canary-1248\napi_key=synthetic-only\n",
    )
    .unwrap();
    let registry = ToolRegistry::new();
    let mut context = ToolContext::new(root.path().into());
    let page = call(
        &registry,
        "Read",
        json!({"file_path":"page.txt","offset":1,"limit":2}),
        context.clone(),
    )
    .await;
    assert!(page.success);
    assert_eq!(page.output, "second\nthird\n");
    let empty = call(
        &registry,
        "Read",
        json!({"file_path":"page.txt","offset":20,"limit":2}),
        context.clone(),
    )
    .await;
    assert!(empty.success && empty.output.is_empty());
    context
        .env
        .insert("FIXTURE_TOKEN".into(), "synthetic-canary-1248".into());
    let secret = call(
        &registry,
        "Read",
        json!({"file_path":"secret.txt"}),
        context.clone(),
    )
    .await;
    assert!(!secret.output.contains("synthetic-canary-1248"));
    assert!(!secret.output.contains("synthetic-only"));
    assert!(!format!("{context:?}").contains("synthetic-canary-1248"));
    let implicit = registry
        .execute("Read", json!({"file_path":"page.txt"}))
        .await
        .unwrap();
    assert!(!implicit.success);
}

#[tokio::test]
async fn security_boundary_paths_require_explicit_opened_roots() {
    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    std::fs::write(outside.path().join("outside.txt"), "outside").unwrap();
    let registry = ToolRegistry::new();
    let context = ToolContext::new(root.path().into()).with_auto_approve(true);
    let path = outside.path().join("outside.txt");
    for name in ["Read", "Create", "Edit"] {
        let args = json!({"file_path":path,"content":"bad","old_str":"outside","new_str":"bad"});
        assert!(!call(&registry, name, args, context.clone()).await.success);
    }
    for (name, args) in [
        ("Read", json!({"file_path":"../outside.txt"})),
        ("LS", json!({"directory_path": outside.path()})),
        ("Grep", json!({"path": outside.path(), "pattern":"outside"})),
        (
            "Glob",
            json!({"folder": outside.path(), "patterns":["**/*"]}),
        ),
        (
            "Execute",
            json!({"command":["echo","bad"],"workdir":outside.path()}),
        ),
        ("Create", json!({"file_path":".git/config","content":"bad"})),
        (
            "Create",
            json!({"file_path":".cortex/settings","content":"bad"}),
        ),
    ] {
        assert!(!call(&registry, name, args, context.clone()).await.success);
    }
    let opened = context.with_opened_root(outside.path().into()).unwrap();
    assert!(
        call(&registry, "Read", json!({"file_path":path}), opened)
            .await
            .success
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), "outside");
}

#[cfg(unix)]
#[tokio::test]
async fn security_boundary_symlink_ancestors_and_patch_are_confined() {
    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    std::fs::write(outside.path().join("data.txt"), "outside").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("missing"), root.path().join("dangling"))
        .unwrap();
    let registry = ToolRegistry::new();
    let context = ToolContext::new(root.path().into()).with_auto_approve(true);
    for (name, args) in [
        ("Read", json!({"file_path":"escape/data.txt"})),
        (
            "Create",
            json!({"file_path":"escape/new/deep/file.txt","content":"bad"}),
        ),
        (
            "Create",
            json!({"file_path":"dangling/new.txt","content":"bad"}),
        ),
        (
            "MultiEdit",
            json!({"edits":[{"file_path":"escape/data.txt","old_str":"outside","new_str":"bad"}]}),
        ),
        (
            "ApplyPatch",
            json!({"patch":"--- /dev/null\n+++ b/escape/new/deep/file.txt\n@@ -0,0 +1 @@\n+bad\n"}),
        ),
    ] {
        assert!(!call(&registry, name, args, context.clone()).await.success);
    }
    let search = call(
        &registry,
        "Grep",
        json!({"pattern":"outside","path":"."}),
        context.clone(),
    )
    .await;
    assert!(search.success && !search.output.contains("data.txt"));
    let glob = call(&registry, "Glob", json!({"patterns":["**/*"]}), context).await;
    assert!(glob.success && !glob.output.contains("data.txt"));
    assert!(!outside.path().join("new").exists());
}

#[tokio::test]
async fn security_boundary_search_errors_are_not_success() {
    let root = tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    std::fs::write(
        root.path().join("src/main.rs"),
        "before\nHello world\nafter\n",
    )
    .unwrap();
    let registry = ToolRegistry::new();
    let context = ToolContext::new(root.path().into());
    for args in [
        json!({"pattern":"[invalid"}),
        json!({"pattern":"ok","path":"missing"}),
    ] {
        assert!(!call(&registry, "Grep", args, context.clone()).await.success);
    }
    let found = call(&registry, "Grep", json!({"pattern":"hello","case_insensitive":true,"glob_pattern":"**/*.rs","output_mode":"content","line_numbers":true,"context":1}), context.clone()).await;
    assert!(found.success, "{}", found.output);
    assert!(
        found.output.contains("2:Hello world")
            && found.output.contains("1:before")
            && found.output.contains("3:after")
    );
    let found = call(
        &registry,
        "Glob",
        json!({"patterns":["src/**/*.rs"]}),
        context,
    )
    .await;
    assert!(found.success && found.output.contains("main.rs"));
}

#[cfg(unix)]
#[tokio::test]
async fn security_boundary_final_child_environment_is_filtered() {
    let root = tempdir().unwrap();
    let registry = ToolRegistry::new();
    let mut context = ToolContext::new(root.path().into())
        .with_sandbox_policy(SandboxPolicy::DangerFullAccess)
        .with_auto_approve(true);
    context
        .env
        .insert("FIXTURE_TOKEN".into(), "synthetic-child-token".into());
    context
        .env
        .insert("FIXTURE_API_KEY".into(), "synthetic-child-key".into());
    context
        .env
        .insert("FIXTURE_PASSWORD".into(), "synthetic-child-password".into());
    context
        .env
        .insert("BASH_ENV".into(), "synthetic-startup-file".into());
    context.env.insert("CI".into(), "false".into());
    let result = call(&registry, "Execute", json!({"command":["/bin/sh","-c",
        "test -z \"$FIXTURE_TOKEN$FIXTURE_API_KEY$FIXTURE_PASSWORD$BASH_ENV\" && test \"$CI\" = true && printf clean"]}), context).await;
    assert!(result.success, "{}", result.output);
    assert_eq!(result.output, "clean");
}

#[cfg(unix)]
#[tokio::test]
async fn security_boundary_plugins_inherit_approval_plan_and_environment() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempdir().unwrap();
    let script = root.path().join("fixture.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\ntest -z \"$FIXTURE_TOKEN\" || exit 9\nprintf plugin-clean\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut registry = ToolRegistry::new();
    registry.register_plugin(PluginTool {
        name: "fixture_plugin".into(),
        description: "fixture".into(),
        parameters: json!({}),
        script_path: script,
    });
    let args = json!({});
    let mut context =
        ToolContext::new(root.path().into()).with_sandbox_policy(SandboxPolicy::DangerFullAccess);
    context
        .env
        .insert("FIXTURE_TOKEN".into(), "synthetic-plugin-token".into());
    assert!(
        !call(&registry, "fixture_plugin", args.clone(), context.clone())
            .await
            .success
    );
    let approved = context.with_approved_tool_call("fixture_plugin", &args);
    assert!(
        !call(
            &registry,
            "fixture_plugin",
            args.clone(),
            approved.clone().with_read_only(true)
        )
        .await
        .success
    );
    let result = call(&registry, "fixture_plugin", args, approved).await;
    assert!(result.success, "{}", result.output);
    assert_eq!(result.output, "plugin-clean");
}

struct SecretHandler;
#[async_trait]
impl ToolHandler for SecretHandler {
    fn name(&self) -> &str {
        "fixture_result"
    }
    async fn execute(&self, _: Value, _: &ToolContext) -> crate::error::Result<ToolResult> {
        Ok(ToolResult::success("synthetic-output-token").with_metadata(
            crate::tools::spec::ToolMetadata {
                duration_ms: 0,
                exit_code: Some(0),
                files_modified: vec!["synthetic-output-token".into()],
                data: Some(
                    json!({"result":"synthetic-output-token","password":"unlabeled-fixture"}),
                ),
            },
        ))
    }
}

#[tokio::test]
async fn security_boundary_dynamic_results_and_metadata_are_redacted_in_batch() {
    let root = tempdir().unwrap();
    let mut registry = ToolRegistry::new();
    registry.register_with_handler(
        ToolDefinition::new("fixture_result", "fixture", json!({})),
        Arc::new(SecretHandler),
    );
    let mut context = ToolContext::new(root.path().into()).with_auto_approve(true);
    context
        .env
        .insert("FIXTURE_TOKEN".into(), "synthetic-output-token".into());
    let result = call(&registry, "fixture_result", json!({}), context.clone()).await;
    assert!(!format!("{result:?}").contains("synthetic-output-token"));
    assert!(!format!("{result:?}").contains("unlabeled-fixture"));
    let batch = call(
        &registry,
        "Batch",
        json!({"calls":[{"tool":"fixture_result","arguments":{}}]}),
        context,
    )
    .await;
    assert!(batch.success && !batch.output.contains("synthetic-output-token"));
}

#[cfg(unix)]
#[tokio::test]
async fn security_boundary_cancellation_kills_controlled_grandchild() {
    let root = tempdir().unwrap();
    let marker = root.path().join("marker");
    let registry = ToolRegistry::new();
    let context = ToolContext::new(root.path().into())
        .with_auto_approve(true)
        .with_sandbox_policy(SandboxPolicy::DangerFullAccess);
    let task = tokio::spawn(async move {
        call(&registry, "Execute", json!({"command":["/bin/sh","-c","(sleep 1; printf leaked > marker) & printf started > started; wait"]}), context).await
    });
    for _ in 0..100 {
        if root.path().join("started").exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        root.path().join("started").exists(),
        "controlled child was not started"
    );
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(!marker.exists(), "grandchild survived cancellation");
}

#[cfg(unix)]
#[tokio::test]
async fn security_boundary_output_is_bounded_and_unicode_safe() {
    let root = tempdir().unwrap();
    std::fs::write(root.path().join("large.txt"), "é".repeat(600_000)).unwrap();
    let registry = ToolRegistry::new();
    let context = ToolContext::new(root.path().into())
        .with_auto_approve(true)
        .with_sandbox_policy(SandboxPolicy::DangerFullAccess);
    let output = call(
        &registry,
        "Execute",
        json!({"command":["cat","large.txt"]}),
        context,
    )
    .await;
    assert!(output.success, "{}", output.output);
    assert!(output.output.len() <= 1024 * 1024 + 64);
    assert!(output.output.contains("truncated"));
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn security_boundary_required_sandbox_does_not_fall_back() {
    let root = tempdir().unwrap();
    // Unit-test executable lives in deps; no sibling sandbox wrapper is installed.
    let registry = ToolRegistry::new();
    let context = ToolContext::new(root.path().into()).with_auto_approve(true);
    let result = call(
        &registry,
        "Execute",
        json!({"command":["touch","must-not-exist"]}),
        context,
    )
    .await;
    assert!(!result.success, "{}", result.output);
    assert!(result.output.contains("sandbox") && result.output.contains("unavailable"));
    assert!(!root.path().join("must-not-exist").exists());
}

#[tokio::test]
async fn security_boundary_structured_and_multiline_secrets_are_redacted() {
    let root = tempdir().unwrap();
    let cases = [
        (
            r#"{"password":"fixture secret with spaces"}"#,
            "fixture secret with spaces",
        ),
        (
            "Authorization: Bearer fixture-bearer-canary",
            "fixture-bearer-canary",
        ),
        (
            "Cookie: first=fixture-cookie-canary; second=fixture-cookie-canary",
            "fixture-cookie-canary",
        ),
        (
            "-----BEGIN PRIVATE KEY-----\nfixture-private-canary\n-----END PRIVATE KEY-----",
            "fixture-private-canary",
        ),
    ];
    let registry = ToolRegistry::new();
    for (text, canary) in cases {
        std::fs::write(root.path().join("fixture.txt"), text).unwrap();
        let result = call(
            &registry,
            "Read",
            json!({"file_path":"fixture.txt"}),
            ToolContext::new(root.path().into()),
        )
        .await;
        assert!(result.success);
        assert!(
            result.output.contains("REDACTED") && !result.output.contains(canary),
            "{}",
            result.output
        );
    }
}

#[tokio::test]
async fn security_boundary_todos_share_only_the_same_workspace_and_conversation() {
    let root = tempdir().unwrap();
    let other_root = tempdir().unwrap();
    let registry = ToolRegistry::new();
    let context = ToolContext::new(root.path().into()).with_conversation_id("first");
    let result = call(&registry, "TodoWrite", json!({"todos":[{"id":"1","content":"fixture-todo","status":"pending","priority":"medium"}]}), context.clone()).await;
    assert!(result.success, "{}", result.output);
    let batch = call(
        &registry.clone(),
        "Batch",
        json!({"calls":[{"tool":"TodoRead","arguments":{}}]}),
        context,
    )
    .await;
    assert!(
        batch.success && batch.output.contains("fixture-todo"),
        "{}",
        batch.output
    );
    for other in [
        ToolContext::new(root.path().into()).with_conversation_id("second"),
        ToolContext::new(other_root.path().into()).with_conversation_id("first"),
    ] {
        let result = call(&registry, "TodoRead", json!({}), other).await;
        assert!(result.success && !result.output.contains("fixture-todo"));
    }
}

#[tokio::test]
async fn security_boundary_unavailable_tools_are_not_advertised_or_started() {
    let root = tempdir().unwrap();
    let registry = ToolRegistry::new();
    let definitions = registry.get_definitions();
    for name in [
        "Task",
        "WebSearch",
        "LspSymbols",
        "LspHover",
        "LspDiagnostics",
    ] {
        assert!(!definitions.iter().any(|tool| tool.name == name));
    }
    let context = ToolContext::new(root.path().into()).with_auto_approve(true);
    let result = call(
        &registry,
        "WebSearch",
        json!({"query":"must not be sent"}),
        context,
    )
    .await;
    assert!(!result.success && result.output.contains("unavailable"));
}

#[tokio::test]
async fn security_boundary_search_bounds_output_before_return() {
    let root = tempdir().unwrap();
    std::fs::write(
        root.path().join("wide.txt"),
        format!("{}\n", "x".repeat(400_000)).repeat(3),
    )
    .unwrap();
    let registry = ToolRegistry::new();
    let context = ToolContext::new(root.path().into());
    let page = call(
        &registry,
        "Grep",
        json!({"pattern":"x", "output_mode":"content", "head_limit":2}),
        context.clone(),
    )
    .await;
    assert!(page.success && page.output.len() < 1024 * 1024);
    let excess = call(
        &registry,
        "Grep",
        json!({"pattern":"x", "output_mode":"content", "head_limit":3}),
        context,
    )
    .await;
    assert!(!excess.success && excess.output.contains("exceeds 1 MiB"));
}
