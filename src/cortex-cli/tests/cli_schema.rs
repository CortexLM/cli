use clap::CommandFactory;
use cortex_cli::cli::Cli;

#[test]
fn test_entire_command_tree_is_valid() {
    Cli::command().debug_assert();
}

#[test]
fn test_plugin_version_and_global_verbosity_are_distinct() {
    let matches = Cli::command()
        .try_get_matches_from([
            "cortex",
            "plugin",
            "install",
            "fixture-plugin",
            "--version",
            "1.2.3",
            "-v",
        ])
        .expect("plugin version must not conflict with global verbosity");
    let install = matches
        .subcommand_matches("plugin")
        .unwrap()
        .subcommand_matches("install")
        .unwrap();
    assert_eq!(install.get_one::<String>("version").unwrap(), "1.2.3");
    assert!(install.get_flag("verbose"));
}
