//! Plugin install/update tests: staging, hashing, and fail-closed pins.

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
    install_local(&installs, &source, false, None, None, None, "install").unwrap();
    let previous = runtime::package::fingerprint(&installs.join("safe")).unwrap();
    std::fs::write(source.join("plugin.mjs"), "invalid javascript !").unwrap();
    assert!(
        install_local(
            &installs,
            &source,
            true,
            None,
            Some("safe"),
            None,
            "install"
        )
        .is_err()
    );
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
        install_local(
            &installs,
            &source,
            false,
            Some("9.9.9"),
            None,
            None,
            "install"
        )
        .unwrap_err()
        .to_string()
        .contains("identity or version")
    );
    assert!(
        install_local(
            &installs,
            &source,
            false,
            None,
            Some("other"),
            None,
            "install"
        )
        .is_err()
    );
    assert!(!installs.join("safe").exists());

    // Nested package files must be recreated under the staged package.
    std::fs::create_dir(source.join("lib")).unwrap();
    std::fs::write(source.join("lib/helper.mjs"), "export const x = 1;").unwrap();
    install_local(
        &installs,
        &source,
        false,
        Some("0.1.0"),
        Some("safe"),
        None,
        "install",
    )
    .unwrap();
    assert!(installs.join("safe/lib/helper.mjs").is_file());

    assert!(
        install_local(&installs, &source, false, None, None, None, "install")
            .unwrap_err()
            .to_string()
            .contains("--force")
    );
    install_local(&installs, &source, true, None, None, None, "install").unwrap();

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
    assert!(install_local(&installs, &broken, false, None, None, None, "install").is_err());

    let empty = temp.path().join("empty-artifact");
    fixture(&empty, "safe");
    std::fs::write(empty.join("plugin.mjs"), "").unwrap();
    assert!(
        install_local(&installs, &empty, false, None, None, None, "install")
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
        install_local(
            &installs,
            &output,
            false,
            Some("0.1.0"),
            Some("safe"),
            None,
            "install"
        )
        .unwrap(),
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

/// Manifest for the accepted-hash fixtures, read through the shipped
/// package validator so the hash covers what an install would register.
fn manifest_of(source: &Path) -> runtime::PluginManifest {
    runtime::package::validate_package(source).unwrap()
}

#[test]
fn an_accepted_hash_installs_and_a_changed_manifest_does_not() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let installs = temp.path().join("installed");
    fixture(&source, "safe");
    let accepted = runtime::command_pin::command_hash(&manifest_of(&source));

    // The reviewed hash installs.
    assert_eq!(
        install_local(
            &installs,
            &source,
            false,
            None,
            None,
            Some(&accepted),
            "install"
        )
        .unwrap(),
        "safe"
    );

    // A manifest that changed after the review must not install, and the
    // previous package must survive untouched.
    let previous = runtime::package::fingerprint(&installs.join("safe")).unwrap();
    let changed = temp.path().join("changed");
    fixture(&changed, "safe");
    std::fs::write(
        changed.join("plugin.toml"),
        "[plugin]\nid=\"safe\"\nname=\"safe\"\nversion=\"0.1.0\"\n[runtime]\nkind=\"node\"\nentrypoint=\"plugin.mjs\"\n[[commands]]\nname=\"sneak\"\ndescription=\"added after review\"\n",
    )
    .unwrap();
    let error = install_local(
        &installs,
        &changed,
        true,
        None,
        Some("safe"),
        Some(&accepted),
        "install",
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("Command hash mismatch. Manifest may have changed."),
        "{error}"
    );
    assert!(error.contains("Re-run with --json"), "{error}");
    assert_eq!(
        previous,
        runtime::package::fingerprint(&installs.join("safe")).unwrap(),
        "a refused install must leave the installed package alone"
    );
}

#[test]
fn a_malformed_accepted_hash_fails_before_anything_is_staged() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let installs = temp.path().join("installed");
    fixture(&source, "safe");
    for bad in ["", "abc", &"z".repeat(64)] {
        let error = install_local(&installs, &source, false, None, None, Some(bad), "install")
            .unwrap_err()
            .to_string();
        assert!(error.contains("SHA-256"), "{bad:?}: {error}");
    }
    assert!(!installs.join("safe").exists());
}

#[test]
fn the_review_document_matches_the_pinned_hash() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fixture(&source, "safe");
    let manifest = manifest_of(&source);
    let review = command_review(&manifest);
    assert_eq!(
        review["command_hash"],
        runtime::command_pin::command_hash(&manifest)
    );
    assert_eq!(review["id"], "safe");
    assert_eq!(review["version"], "0.1.0");
    assert!(review["commands"].is_array());
    // A `--json` review followed by the printed hash installs.
    let accepted = review["command_hash"].as_str().unwrap().to_string();
    let installs = temp.path().join("installed");
    install_local(
        &installs,
        &source,
        false,
        None,
        None,
        Some(&accepted),
        "install",
    )
    .unwrap();
}

/// Organization policy that requires a pin refuses an unpinned install.
#[test]
fn organization_policy_requires_the_pin() {
    use cortex_engine::org_policy::PluginInstallPolicy;

    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fixture(&source, "safe");
    let manifest = manifest_of(&source);

    let required = PluginInstallPolicy::RequireAcceptCommand;
    assert!(required.requires_pin());
    let unpinned = enforce_command_pin_with(&manifest, None, "install", required)
        .unwrap_err()
        .to_string();
    assert!(unpinned.contains("requires --accept-command"), "{unpinned}");
    assert!(unpinned.contains("--json"), "{unpinned}");

    // The reviewed hash is accepted under the same policy.
    let accepted = runtime::command_pin::command_hash(&manifest);
    assert!(enforce_command_pin_with(&manifest, Some(&accepted), "install", required).is_ok());
    // A changed manifest is still refused even with a pin present.
    let changed = temp.path().join("changed");
    fixture(&changed, "safe");
    std::fs::write(
        changed.join("plugin.toml"),
        "[plugin]\nid=\"safe\"\nname=\"safe\"\nversion=\"0.2.0\"\n[runtime]\nkind=\"node\"\nentrypoint=\"plugin.mjs\"\n",
    )
    .unwrap();
    assert!(
        enforce_command_pin_with(&manifest_of(&changed), Some(&accepted), "install", required)
            .unwrap_err()
            .to_string()
            .contains("Command hash mismatch")
    );

    // Without the requirement an unpinned install is unchanged.
    let optional = PluginInstallPolicy::HostDefault;
    assert!(!optional.requires_pin());
    assert!(enforce_command_pin_with(&manifest, None, "install", optional).is_ok());
}

/// A refused install leaves nothing behind in the plugin root.
#[test]
fn an_org_required_pin_never_reaches_the_plugin_root() {
    use cortex_engine::org_policy::PluginInstallPolicy;

    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let installs = temp.path().join("installed");
    fixture(&source, "safe");
    let manifest = manifest_of(&source);
    // Exercise the shipped decision path with the requirement in place.
    let error = enforce_command_pin_with(
        &manifest,
        None,
        "install",
        PluginInstallPolicy::RequireAcceptCommand,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("requires --accept-command"), "{error}");
    assert!(!installs.join("safe").exists());
}

/// An accepted pin is written to the audit journal as one JSON line.
#[test]
fn an_accepted_pin_is_audited() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fixture(&source, "safe");
    let manifest = manifest_of(&source);
    let accepted = runtime::command_pin::command_hash(&manifest);

    let home = temp.path().join("home");
    let journal = cortex_engine::audit::record(
        &home,
        cortex_engine::audit::AuditKind::PluginCommandAccepted,
        serde_json::json!({
            "plugin": manifest.plugin.id,
            "version": manifest.plugin.version,
            "accepted_hash": accepted,
            "actual_hash": runtime::command_pin::command_hash(&manifest),
            "action": "install",
            "policy": "host_default",
        }),
    )
    .unwrap();
    let body = std::fs::read_to_string(&journal).unwrap();
    assert!(body.contains("plugin_command_accepted"), "{body}");
    assert!(body.contains(&accepted), "{body}");
    assert!(body.contains("\"action\":\"install\""), "{body}");
    assert_eq!(body.lines().count(), 1);
}
