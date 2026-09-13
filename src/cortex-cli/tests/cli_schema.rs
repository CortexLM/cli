use clap::CommandFactory;
use cortex_cli::cli::Cli;

#[test]
fn test_entire_command_tree_is_valid() {
    Cli::command().debug_assert();
}

#[test]
fn test_development_commands_remain_available_without_admin_commands() {
    let command = Cli::command();
    for name in [
        "run",
        "exec",
        "resume",
        "sessions",
        "export",
        "import",
        "delete",
        "attach",
        "jobs",
        "login",
        "logout",
        "whoami",
        "agent",
        "mcp",
        "mcp-server",
        "acp",
        "config",
        "models",
        "features",
        "init",
        "github",
        "pr",
        "scrape",
        "stats",
        "completion",
        "upgrade",
        "uninstall",
        "compact",
        "cache",
        "logs",
        "feedback",
        "lock",
        "alias",
        "plugin",
        "debug",
        "shell",
        "dag",
        "servers",
        "history",
        "workspace",
        "sandbox",
        "serve",
    ] {
        assert!(command.find_subcommand(name).is_some(), "missing {name}");
    }
    fn check_no_admin(command: &clap::Command) {
        assert_ne!(command.get_name(), "admin");
        assert!(!command.get_all_aliases().any(|alias| alias == "admin"));
        for child in command.get_subcommands() {
            check_no_admin(child);
        }
    }
    check_no_admin(&command);
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
