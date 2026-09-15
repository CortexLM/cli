//! Command-hash pinning for plugin installs.
//!
//! `plugin install` and `plugin update` can print the exact commands a package
//! would register. `--accept-command <sha256>` pins that review: the install
//! proceeds only when the package under install hashes to the same value.
//!
//! The hash covers the plugin id, version, and every declared command with its
//! aliases, arguments, and flags, so a manifest that changed in any way that a
//! user could notice produces a different value. A mismatch fails closed; there
//! is no `-y` shortcut and no automatic re-acceptance.

use sha2::{Digest, Sha256};

use crate::{PluginError, PluginManifest, Result};

/// Product copy for a hash that does not match the reviewed package.
pub const HASH_MISMATCH: &str =
    "Command hash mismatch. Manifest may have changed. Re-run with --json and accept the new hash.";
/// Product copy for a hash that is not a SHA-256 value.
pub const HASH_INVALID: &str =
    "Accept-command hash must be a 64-character SHA-256 value from --json.";

/// Length-prefix a free-text field so unrestricted values cannot collide
/// across `|` (or other) separators in the review digest.
fn field(label: &str, value: &str) -> String {
    format!("{label}:{}\n{value}\n", value.len())
}

/// Canonical, stable description of the commands a manifest declares.
///
/// One entry per command, in declaration order, with every user-visible field
/// that a review would show. Free text is length-prefixed so distinct metadata
/// cannot hash-collide. Hooks include type, priority, pattern, and function.
pub fn command_review(manifest: &PluginManifest) -> String {
    let mut out = format!(
        "plugin {} {}\n",
        manifest.plugin.id, manifest.plugin.version
    );
    for command in &manifest.commands {
        out.push_str("command\n");
        out.push_str(&field("name", &command.name));
        out.push_str(&field("description", &command.description));
        out.push_str(&field(
            "usage",
            command.usage.as_deref().unwrap_or(""),
        ));
        out.push_str(&format!("hidden={}\n", command.hidden));
        for alias in &command.aliases {
            out.push_str(&field("alias", alias));
        }
        for arg in &command.args {
            out.push_str("arg\n");
            out.push_str(&field("name", &arg.name));
            out.push_str(&format!("required={}\n", arg.required));
            out.push_str(&field(
                "default",
                arg.default.as_deref().unwrap_or(""),
            ));
        }
    }
    for hook in &manifest.hooks {
        out.push_str("hook\n");
        out.push_str(&field("type", &hook.hook_type.to_string()));
        out.push_str(&format!("priority={}\n", hook.priority));
        out.push_str(&field(
            "pattern",
            hook.pattern.as_deref().unwrap_or(""),
        ));
        out.push_str(&field(
            "function",
            hook.function.as_deref().unwrap_or(""),
        ));
    }
    for tool in &manifest.tools {
        out.push_str(&field("tool", &tool.name));
    }
    out
}

/// SHA-256 of the canonical command review.
pub fn command_hash(manifest: &PluginManifest) -> String {
    hex::encode(Sha256::digest(command_review(manifest).as_bytes()))
}

/// True when `value` is a well-formed SHA-256 hex string.
pub fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Verify a reviewed hash against the package that is about to install.
///
/// Fails closed: an invalid hash and a mismatched hash are both errors, and the
/// error names the exact recovery step.
pub fn verify_command_hash(manifest: &PluginManifest, accepted: &str) -> Result<()> {
    if !is_sha256(accepted) {
        return Err(PluginError::validation_error(
            "accept-command",
            HASH_INVALID,
        ));
    }
    let actual = command_hash(manifest);
    if actual != accepted.to_ascii_lowercase() {
        return Err(PluginError::validation_error(
            "accept-command",
            HASH_MISMATCH,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"
[plugin]
id = "review"
name = "Review"
version = "1.2.0"

[runtime]
kind = "node"
entrypoint = "plugin.mjs"

[[commands]]
name = "review"
description = "Review the working tree"
usage = "/review [path]"
ALIASES

[[commands.args]]
name = "path"
required = false
"#;

    fn manifest(body: &str) -> PluginManifest {
        PluginManifest::parse(body).expect("fixture parses")
    }

    /// Fill the fixture's command-table marker with extra command fields.
    fn with_command_fields(fields: &str) -> PluginManifest {
        manifest(&BASE.replace("ALIASES", fields))
    }

    #[test]
    fn the_hash_is_stable_and_hex_sha256() {
        let hash = command_hash(&with_command_fields(""));
        assert!(is_sha256(&hash), "{hash}");
        assert_eq!(hash, command_hash(&with_command_fields("")));
        assert_eq!(hash, hash.to_ascii_lowercase());
    }

    #[test]
    fn any_reviewed_change_moves_the_hash() {
        let base = command_hash(&with_command_fields(""));
        let renamed = command_hash(&manifest(
            &BASE
                .replace("ALIASES", "")
                .replace("name = \"review\"", "name = \"check\""),
        ));
        assert_ne!(base, renamed, "a renamed command must change the hash");
        let described = command_hash(&manifest(&BASE.replace("ALIASES", "").replace(
            "description = \"Review the working tree\"",
            "description = \"Review a diff\"",
        )));
        assert_ne!(base, described);
        let versioned = command_hash(&manifest(
            &BASE
                .replace("ALIASES", "")
                .replace("version = \"1.2.0\"", "version = \"1.2.1\""),
        ));
        assert_ne!(base, versioned, "a version bump must change the hash");
        let argued = command_hash(&manifest(
            &BASE
                .replace("ALIASES", "")
                .replace("required = false", "required = true"),
        ));
        assert_ne!(base, argued);
        let hidden = command_hash(&with_command_fields("hidden = true"));
        assert_ne!(base, hidden);
    }

    #[test]
    fn an_alias_only_change_still_moves_the_hash() {
        let base = command_hash(&with_command_fields(""));
        let aliased = command_hash(&with_command_fields("aliases = [\"rev\"]"));
        assert_ne!(base, aliased);
    }
    #[test]
    fn a_matching_hash_is_accepted_in_either_case() {
        let review = with_command_fields("");
        let hash = command_hash(&review);
        assert!(verify_command_hash(&review, &hash).is_ok());
        assert!(verify_command_hash(&review, &hash.to_ascii_uppercase()).is_ok());
    }

    #[test]
    fn a_mismatch_fails_closed_with_the_review_copy() {
        let review = with_command_fields("");
        let other = manifest(
            &BASE
                .replace("ALIASES", "")
                .replace("version = \"1.2.0\"", "version = \"2.0.0\""),
        );
        let error = verify_command_hash(&other, &command_hash(&review))
            .expect_err("a changed manifest must not install")
            .to_string();
        assert!(error.contains(HASH_MISMATCH), "{error}");
        assert!(error.contains("Re-run with --json"), "{error}");
    }

    #[test]
    fn a_malformed_hash_is_rejected_before_any_work() {
        let review = with_command_fields("");
        for bad in ["", "abc", &"z".repeat(64), &"a".repeat(63)] {
            let error = verify_command_hash(&review, bad)
                .expect_err("a malformed hash must not install")
                .to_string();
            assert!(error.contains("SHA-256"), "{bad:?}: {error}");
        }
    }

    #[test]
    fn the_review_names_every_declared_surface() {
        let review = command_review(&with_command_fields(""));
        assert!(review.contains("plugin review 1.2.0"), "{review}");
        assert!(review.contains("name:6\nreview\n"), "{review}");
        assert!(review.contains("/review [path]"), "{review}");
        assert!(review.contains("name:4\npath\n"), "{review}");
    }

    #[test]
    fn free_text_cannot_collide_across_fields() {
        // Distinct description/usage pairs that would collide under raw `|`
        // joins must produce different hashes with length-prefixed fields.
        let a = manifest(
            r#"
[plugin]
id = "review"
name = "Review"
version = "1.0.0"

[runtime]
kind = "node"
entrypoint = "plugin.mjs"

[[commands]]
name = "x"
description = "ab|c"
usage = "d"
"#,
        );
        let b = manifest(
            r#"
[plugin]
id = "review"
name = "Review"
version = "1.0.0"

[runtime]
kind = "node"
entrypoint = "plugin.mjs"

[[commands]]
name = "x"
description = "ab"
usage = "c|d"
"#,
        );
        assert_ne!(command_hash(&a), command_hash(&b));
        assert_ne!(command_review(&a), command_review(&b));
    }

    #[test]
    fn hook_executable_fields_move_the_hash() {
        let base_body = r#"
[plugin]
id = "review"
name = "Review"
version = "1.0.0"

[runtime]
kind = "node"
entrypoint = "plugin.mjs"

[[hooks]]
hook_type = "session_start"
priority = 10
pattern = "*.rs"
function = "on_start"
"#;
        let base = command_hash(&manifest(base_body));
        let changed_fn = command_hash(&manifest(
            &base_body.replace("function = \"on_start\"", "function = \"other\""),
        ));
        assert_ne!(base, changed_fn, "hook function must be pinned");
        let changed_pri = command_hash(&manifest(
            &base_body.replace("priority = 10", "priority = 99"),
        ));
        assert_ne!(base, changed_pri, "hook priority must be pinned");
        let changed_pat = command_hash(&manifest(
            &base_body.replace("pattern = \"*.rs\"", "pattern = \"*.toml\""),
        ));
        assert_ne!(base, changed_pat, "hook pattern must be pinned");
    }
}
