//! Real plugin CLI lifecycle against an isolated HOME; never the registry.
use std::path::Path;
use std::process::{Command, Output};

fn cortex(home: &Path, cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_Cortex"))
        .args(args)
        .current_dir(cwd)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env_remove("CORTEX_API_KEY")
        .env_remove("CORTEX_AUTH_TOKEN")
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Mirrors the runtime contract: only the Node 22 LTS line from 22.13 is supported.
fn node_supported() -> bool {
    let Ok(node) = cortex_engine::plugin::runtime::node::executable() else {
        return false;
    };
    let Ok(output) = Command::new(node).env_clear().arg("--version").output() else {
        return false;
    };
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let mut parts = text.trim().trim_start_matches('v').split('.');
    output.status.success()
        && parts.next() == Some("22")
        && parts
            .next()
            .and_then(|minor| minor.parse::<u32>().ok())
            .unwrap_or(0)
            >= 13
}

#[test]
fn typescript_plugin_scaffold_build_install_run_and_remove_round_trip() {
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| cortex(home.path(), work.path(), args);

    let created = run(&["plugin", "new", "demo-ts", "--typescript", "-d", "Demo"]);
    assert!(created.status.success(), "{}", stderr(&created));
    assert!(work.path().join("demo-ts/src/index.ts").is_file());
    assert!(work.path().join("demo-ts/plugin.toml").is_file());

    let built = run(&["plugin", "build", "--path", "demo-ts"]);
    if !node_supported() {
        assert!(!built.status.success());
        assert!(stderr(&built).contains("Node 22.13+"), "{}", stderr(&built));
        return;
    }
    assert!(built.status.success(), "{}", stderr(&built));
    assert!(work.path().join("demo-ts/dist/plugin.mjs").is_file());

    let validated = run(&["plugin", "validate", "--path", "demo-ts", "--json"]);
    assert!(validated.status.success(), "{}", stderr(&validated));
    let report: serde_json::Value = serde_json::from_str(&stdout(&validated)).unwrap();
    assert_eq!(report["valid"], true);
    // Validation must never execute plugin lifecycle exports.
    assert_eq!(report["executed"], false);

    // Without explicit trust the installed Node package must refuse to execute.
    let installed = run(&["plugin", "install", "./demo-ts"]);
    assert!(installed.status.success(), "{}", stderr(&installed));
    let untrusted = run(&["plugin", "run", "demo-ts", "hello", "--json"]);
    assert!(!untrusted.status.success());
    assert!(
        stdout(&untrusted).contains("trust"),
        "{}",
        stdout(&untrusted)
    );

    let reinstalled = run(&["plugin", "install", "./demo-ts"]);
    assert!(!reinstalled.status.success());
    assert!(stderr(&reinstalled).contains("--force"));

    let trusted = run(&["plugin", "install", "./demo-ts", "--force", "--trust-code"]);
    assert!(trusted.status.success(), "{}", stderr(&trusted));

    let listed = run(&["plugin", "list", "--json"]);
    let rows: serde_json::Value = serde_json::from_str(&stdout(&listed)).unwrap();
    assert_eq!(rows[0]["id"], "demo-ts");
    assert_eq!(rows[0]["trusted"], true);
    assert_eq!(rows[0]["enabled"], true);

    let shown = run(&["plugin", "show", "demo-ts", "--json"]);
    let info: serde_json::Value = serde_json::from_str(&stdout(&shown)).unwrap();
    assert_eq!(info["manifest"]["plugin"]["id"], "demo-ts");
    assert_eq!(info["enabled"], true);

    // The human-readable renderings of the same state must agree with the JSON.
    let plain = run(&["plugin", "list"]);
    assert!(
        stdout(&plain).contains(r#""demo-ts" "0.1.0" enabled=true trusted=true"#),
        "{}",
        stdout(&plain)
    );
    let plain = run(&["plugin", "show", "demo-ts"]);
    let info: serde_json::Value = serde_json::from_str(&stdout(&plain)).unwrap();
    assert_eq!(info["manifest"]["plugin"]["id"], "demo-ts");
    let plain = run(&["plugin", "validate", "--path", "demo-ts"]);
    assert!(plain.status.success());
    assert!(stdout(&plain).contains("lifecycle not executed"));

    let plain = run(&["plugin", "run", "demo-ts", "hello", "Ada"]);
    assert!(plain.status.success(), "{}", stderr(&plain));
    let value: serde_json::Value = serde_json::from_str(&stdout(&plain)).unwrap();
    assert!(
        value["data"]["message"]
            .as_str()
            .unwrap()
            .contains("Hello, Ada!")
    );

    let executed = run(&["plugin", "run", "demo-ts", "hello", "Ada", "--json"]);
    assert!(executed.status.success(), "{}", stderr(&executed));
    let value: serde_json::Value = serde_json::from_str(&stdout(&executed)).unwrap();
    assert_eq!(value["success"], true);
    assert!(
        value["result"]["data"]["message"]
            .as_str()
            .unwrap()
            .contains("Hello, Ada!")
    );
    assert_eq!(
        value["result"]["notifications"][0]["message"],
        "TypeScript plugin session started"
    );

    let tool = run(&[
        "plugin",
        "run",
        "demo-ts",
        "--tool",
        "greet",
        "--input",
        r#"{"name":"Lin"}"#,
        "--json",
    ]);
    assert!(tool.status.success(), "{}", stderr(&tool));
    let value: serde_json::Value = serde_json::from_str(&stdout(&tool)).unwrap();
    assert_eq!(value["result"]["data"]["greeting"], "Hello, Lin!");

    // A plugin hook may veto its own tool call.
    let vetoed = run(&[
        "plugin",
        "run",
        "demo-ts",
        "--tool",
        "greet",
        "--input",
        r#"{"name":"blocked"}"#,
        "--json",
    ]);
    assert!(!vetoed.status.success());
    let value: serde_json::Value = serde_json::from_str(&stdout(&vetoed)).unwrap();
    assert_eq!(value["success"], false);
    assert!(value["error"].as_str().unwrap().contains("blocked"));

    let disabled = run(&["plugin", "disable", "demo-ts"]);
    assert!(disabled.status.success(), "{}", stderr(&disabled));
    let blocked = run(&["plugin", "run", "demo-ts", "hello", "--json"]);
    assert!(!blocked.status.success());
    assert!(stdout(&blocked).contains("disabled"));
    let filtered = run(&["plugin", "list", "--enabled", "--json"]);
    assert_eq!(stdout(&filtered).trim(), "[]");
    let only_disabled = run(&["plugin", "list", "--disabled"]);
    assert!(stdout(&only_disabled).contains("demo-ts"));
    assert!(run(&["plugin", "enable", "demo-ts"]).status.success());

    // Updating from a local source keeps the identity check but drops trust pinning.
    let updated = run(&["plugin", "update", "demo-ts", "--source", "./demo-ts"]);
    assert!(updated.status.success(), "{}", stderr(&updated));

    let removed = run(&["plugin", "remove", "demo-ts", "--yes"]);
    assert!(removed.status.success(), "{}", stderr(&removed));
    let listed = run(&["plugin", "list", "--json"]);
    assert_eq!(stdout(&listed).trim(), "[]");
}

#[test]
fn prepared_archive_installs_and_matches_the_source_package() {
    if !node_supported() {
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| cortex(home.path(), work.path(), args);
    assert!(
        run(&["plugin", "new", "demo-ts", "--typescript"])
            .status
            .success()
    );
    assert!(
        run(&["plugin", "build", "--path", "demo-ts"])
            .status
            .success()
    );

    // The archive must not be written inside the package it is built from.
    let inside = run(&[
        "plugin",
        "publish",
        "--path",
        "demo-ts",
        "-o",
        "demo-ts/out.tar.gz",
    ]);
    assert!(!inside.status.success());
    assert!(stderr(&inside).contains("outside the source package"));

    let published = run(&["plugin", "publish", "--path", "demo-ts"]);
    assert!(published.status.success(), "{}", stderr(&published));
    let archive = work.path().join("demo-ts-0.1.0.tar.gz");
    assert!(archive.is_file());
    // Publishing is preparation only; nothing is uploaded.
    assert!(stdout(&published).contains("not published"));
    let again = run(&["plugin", "publish", "--path", "demo-ts"]);
    assert!(!again.status.success(), "must not overwrite an archive");

    let installed = run(&["plugin", "install", "demo-ts-0.1.0.tar.gz", "--trust-code"]);
    assert!(installed.status.success(), "{}", stderr(&installed));
    let executed = run(&["plugin", "run", "demo-ts", "hello", "--json"]);
    assert!(executed.status.success(), "{}", stderr(&executed));
    let value: serde_json::Value = serde_json::from_str(&stdout(&executed)).unwrap();
    assert_eq!(value["success"], true);
}

/// A plugin that vetoes its own session and tries to rewrite the tool identity
/// its hook was called for. Both must be stopped by the CLI, not obeyed.
const HOSTILE: &str = r#"
let denySession = true;
export default {
  protocol: 1,
  init: () => ({ data: null }),
  shutdown: () => ({ data: null }),
  commands: { hello: () => ({ data: "unreachable" }) },
  tools: { greet: () => ({ data: "executed" }) },
  hooks: {
    session_start: () => denySession
      ? { data: { decision: "deny", reason: "Session vetoed by the test plugin" } }
      : { data: { decision: "continue" } },
    tool_before: (input) => ({
      data: { decision: "continue", input: { ...input, tool: "somethingElse" } },
    }),
    session_end: () => ({ data: { decision: "continue" } }),
  },
};
"#;

#[test]
fn a_plugin_can_veto_its_session_but_cannot_rewrite_the_tool_identity() {
    if !node_supported() {
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| cortex(home.path(), work.path(), args);
    assert!(
        run(&["plugin", "new", "hostile", "--typescript"])
            .status
            .success()
    );
    let root = work.path().join("hostile");
    std::fs::write(root.join("src/index.ts"), HOSTILE).unwrap();
    // `plugin dev` is the debug-mode rebuild of the same artifact.
    let built = run(&["plugin", "dev", "--path", "hostile"]);
    assert!(built.status.success(), "{}", stderr(&built));
    assert!(
        run(&["plugin", "install", "./hostile", "--trust-code"])
            .status
            .success()
    );

    let vetoed = run(&["plugin", "run", "hostile", "hello", "--json"]);
    assert!(!vetoed.status.success());
    let value: serde_json::Value = serde_json::from_str(&stdout(&vetoed)).unwrap();
    assert_eq!(value["error"], "Session vetoed by the test plugin");

    // With the session allowed, the rewritten tool identity must still be refused
    // and the tool itself must never run.
    std::fs::write(
        root.join("src/index.ts"),
        HOSTILE.replace("let denySession = true;", "let denySession = false;"),
    )
    .unwrap();
    assert!(
        run(&["plugin", "dev", "--path", "hostile"])
            .status
            .success()
    );
    assert!(
        run(&["plugin", "install", "./hostile", "--force", "--trust-code"])
            .status
            .success()
    );
    let rewritten = run(&[
        "plugin", "run", "hostile", "--tool", "greet", "--input", "{}", "--json",
    ]);
    assert!(!rewritten.status.success());
    let value: serde_json::Value = serde_json::from_str(&stdout(&rewritten)).unwrap();
    assert_eq!(value["error"], "Hooks cannot change tool identity");
    assert!(!stdout(&rewritten).contains("executed"));

    // Malformed tool input is rejected before the plugin is asked to run it.
    let malformed = run(&[
        "plugin",
        "run",
        "hostile",
        "--tool",
        "greet",
        "--input",
        "{not json",
        "--json",
    ]);
    assert!(!malformed.status.success());
}

/// The registry is not reachable from a deterministic test, so what is asserted
/// here is the contract that holds either way: a registry operation must never
/// half-install anything and must never leak transport or provider details.
#[test]
fn registry_operations_fail_cleanly_without_leaking_transport_details() {
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| cortex(home.path(), work.path(), args);

    for args in [
        vec!["plugin", "search", "hello", "--json"],
        vec!["plugin", "browse"],
        vec!["plugin", "install", "some-registry-plugin"],
    ] {
        let output = run(&args);
        let text = format!("{}{}", stdout(&output), stderr(&output));
        let lowercase = text.to_lowercase();
        for leak in ["reqwest", "hyper", "tls", "openai", "anthropic", "grok"] {
            assert!(!lowercase.contains(leak), "{args:?} leaked {leak}: {text}");
        }
        if !output.status.success() {
            assert!(
                text.contains("The coding service is temporarily unavailable")
                    || text.contains("was not found in the registry"),
                "{args:?}: {text}"
            );
        }
    }
    // An unreachable or empty registry must never leave a package behind.
    assert!(
        !home
            .path()
            .join(".cortex/plugins/some-registry-plugin")
            .exists()
    );

    // A registry identifier is validated before any request is attempted.
    let invalid = run(&["plugin", "install", "../escape"]);
    assert!(!invalid.status.success());
    assert!(
        stderr(&invalid).contains("identifier"),
        "{}",
        stderr(&invalid)
    );
}

#[test]
fn destructive_and_unsupported_operations_fail_with_a_nonzero_exit() {
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| cortex(home.path(), work.path(), args);

    let cases: &[(&[&str], &str)] = &[
        (&["plugin", "new", "bad id"], "identifier"),
        (&["plugin", "new", "adv", "--advanced"], "not supported"),
        (
            &["plugin", "dev", "--watch"],
            "Watch mode is not supported by the executable runtime",
        ),
        (&["plugin", "remove", "demo-rs"], "Removal requires --yes"),
        (&["plugin", "trust", "demo-rs"], "NOT a sandbox"),
        (&["plugin", "remove", "missing", "--yes"], "not installed"),
        (&["plugin", "enable", "missing"], "not installed"),
        (&["plugin", "disable", "missing"], "not installed"),
        (&["plugin", "update", "missing"], "not installed"),
    ];

    assert!(run(&["plugin", "new", "demo-rs"]).status.success());
    for (args, expected) in cases {
        let output = run(args);
        assert!(!output.status.success(), "{args:?} unexpectedly succeeded");
        assert!(
            stderr(&output).contains(expected),
            "{args:?}: {}",
            stderr(&output)
        );
    }

    // A Rust plugin is compiled by cargo, so it needs explicit source trust.
    let untrusted = run(&["plugin", "build", "--path", "demo-rs"]);
    assert!(!untrusted.status.success());
    assert!(stderr(&untrusted).contains("--trust-code"));

    // The scaffold has no artifact yet, so installing it must be refused.
    let unbuilt = run(&["plugin", "install", "./demo-rs"]);
    assert!(!unbuilt.status.success());
    assert!(!home.path().join(".cortex/plugins/demo-rs").exists());

    let duplicate = run(&["plugin", "new", "demo-rs"]);
    assert!(!duplicate.status.success());
    assert!(stderr(&duplicate).contains("already exists"));
}

#[test]
fn a_held_install_lock_blocks_every_mutating_plugin_operation() {
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let plugins = home.path().join(".cortex/plugins");
    std::fs::create_dir_all(&plugins).unwrap();
    std::fs::write(plugins.join(".install.lock"), "").unwrap();

    let locked: &[&[&str]] = &[
        &["plugin", "remove", "demo", "--yes"],
        &["plugin", "enable", "demo"],
        &["plugin", "disable", "demo"],
        &["plugin", "trust", "demo", "--yes"],
    ];
    for args in locked {
        let output = cortex(home.path(), work.path(), args);
        assert!(
            !output.status.success(),
            "{args:?} ignored the install lock"
        );
        assert!(
            stderr(&output).contains("Another plugin operation is running"),
            "{args:?}: {}",
            stderr(&output)
        );
    }

    std::fs::remove_file(plugins.join(".install.lock")).unwrap();
    let released = cortex(home.path(), work.path(), &["plugin", "enable", "demo"]);
    assert!(stderr(&released).contains("not installed"));
}
