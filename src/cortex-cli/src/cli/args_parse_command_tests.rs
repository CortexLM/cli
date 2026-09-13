use super::*;
use clap::Parser;

// ==========================================================================
// SessionsCommand tests
// ==========================================================================

#[test]
fn test_sessions_command_default() {
    let cli = Cli::try_parse_from(["cortex", "sessions"]).expect("should parse sessions");
    if let Some(Commands::Sessions(sessions)) = cli.command {
        assert!(!sessions.all);
        assert!(sessions.days.is_none());
        assert!(sessions.since.is_none());
        assert!(sessions.until.is_none());
        assert!(!sessions.favorites);
        assert!(sessions.search.is_none());
        assert!(sessions.limit.is_none());
        assert!(!sessions.json);
    } else {
        panic!("Expected Sessions command");
    }
}

#[test]
fn test_sessions_command_with_flags() {
    let cli = Cli::try_parse_from([
        "cortex",
        "sessions",
        "--all",
        "--days",
        "7",
        "--favorites",
        "--limit",
        "10",
        "--json",
    ])
    .expect("should parse sessions with flags");
    if let Some(Commands::Sessions(sessions)) = cli.command {
        assert!(sessions.all);
        assert_eq!(sessions.days, Some(7));
        assert!(sessions.favorites);
        assert_eq!(sessions.limit, Some(10));
        assert!(sessions.json);
    } else {
        panic!("Expected Sessions command");
    }
}

#[test]
fn test_sessions_command_search() {
    let cli = Cli::try_parse_from(["cortex", "sessions", "--search", "fix bug"])
        .expect("should parse sessions --search");
    if let Some(Commands::Sessions(sessions)) = cli.command {
        assert_eq!(sessions.search, Some("fix bug".to_string()));
    } else {
        panic!("Expected Sessions command");
    }
}

// ==========================================================================
// DeleteCommand tests
// ==========================================================================

#[test]
fn test_delete_command() {
    let cli = Cli::try_parse_from(["cortex", "delete", "abc12345"])
        .expect("should parse delete with session id");
    if let Some(Commands::Delete(delete)) = cli.command {
        assert_eq!(delete.session_id, "abc12345");
        assert!(!delete.yes);
        assert!(!delete.force);
    } else {
        panic!("Expected Delete command");
    }
}

#[test]
fn test_delete_command_with_flags() {
    let cli = Cli::try_parse_from(["cortex", "delete", "abc12345", "-y", "-f"])
        .expect("should parse delete with flags");
    if let Some(Commands::Delete(delete)) = cli.command {
        assert_eq!(delete.session_id, "abc12345");
        assert!(delete.yes);
        assert!(delete.force);
    } else {
        panic!("Expected Delete command");
    }
}

// ==========================================================================
// ConfigCommand tests
// ==========================================================================

#[test]
fn test_config_command_default() {
    let cli = Cli::try_parse_from(["cortex", "config"]).expect("should parse config");
    if let Some(Commands::Config(config)) = cli.command {
        assert!(!config.json);
        assert!(!config.edit);
        assert!(config.action.is_none());
    } else {
        panic!("Expected Config command");
    }
}

#[test]
fn test_config_command_json() {
    let cli =
        Cli::try_parse_from(["cortex", "config", "--json"]).expect("should parse config --json");
    if let Some(Commands::Config(config)) = cli.command {
        assert!(config.json);
    } else {
        panic!("Expected Config command");
    }
}

#[test]
fn test_config_command_edit() {
    let cli =
        Cli::try_parse_from(["cortex", "config", "--edit"]).expect("should parse config --edit");
    if let Some(Commands::Config(config)) = cli.command {
        assert!(config.edit);
    } else {
        panic!("Expected Config command");
    }
}

#[test]
fn test_config_get_subcommand() {
    let cli =
        Cli::try_parse_from(["cortex", "config", "get", "model"]).expect("should parse config get");
    if let Some(Commands::Config(config)) = cli.command {
        if let Some(ConfigSubcommand::Get(get)) = config.action {
            assert_eq!(get.key, "model");
        } else {
            panic!("Expected Get subcommand");
        }
    } else {
        panic!("Expected Config command");
    }
}

#[test]
fn test_config_set_subcommand() {
    let cli = Cli::try_parse_from(["cortex", "config", "set", "model", "gpt-4o"])
        .expect("should parse config set");
    if let Some(Commands::Config(config)) = cli.command {
        if let Some(ConfigSubcommand::Set(set)) = config.action {
            assert_eq!(set.key, "model");
            assert_eq!(set.value, "gpt-4o");
        } else {
            panic!("Expected Set subcommand");
        }
    } else {
        panic!("Expected Config command");
    }
}

#[test]
fn test_config_unset_subcommand() {
    let cli = Cli::try_parse_from(["cortex", "config", "unset", "api_key"])
        .expect("should parse config unset");
    if let Some(Commands::Config(config)) = cli.command {
        if let Some(ConfigSubcommand::Unset(unset)) = config.action {
            assert_eq!(unset.key, "api_key");
        } else {
            panic!("Expected Unset subcommand");
        }
    } else {
        panic!("Expected Config command");
    }
}

// ==========================================================================
// ServeCommand tests
// ==========================================================================

#[test]
fn test_serve_command_default() {
    let cli = Cli::try_parse_from(["cortex", "serve"]).expect("should parse serve");
    if let Some(Commands::Serve(serve)) = cli.command {
        assert_eq!(serve.port, 3000);
        assert_eq!(serve.host, "127.0.0.1");
        assert!(serve.auth_token.is_none());
        assert!(!serve.cors);
        assert!(serve.cors_origins.is_empty());
        assert!(!serve.mdns);
        assert!(!serve.no_mdns);
        assert!(serve.mdns_name.is_none());
    } else {
        panic!("Expected Serve command");
    }
}

#[test]
fn test_serve_command_with_port() {
    let cli = Cli::try_parse_from(["cortex", "serve", "--port", "8080"])
        .expect("should parse serve --port");
    if let Some(Commands::Serve(serve)) = cli.command {
        assert_eq!(serve.port, 8080);
    } else {
        panic!("Expected Serve command");
    }
}

#[test]
fn test_serve_command_with_host() {
    let cli = Cli::try_parse_from(["cortex", "serve", "--host", "0.0.0.0"])
        .expect("should parse serve --host");
    if let Some(Commands::Serve(serve)) = cli.command {
        assert_eq!(serve.host, "0.0.0.0");
    } else {
        panic!("Expected Serve command");
    }
}

#[test]
fn test_serve_command_cors() {
    let cli =
        Cli::try_parse_from(["cortex", "serve", "--cors"]).expect("should parse serve --cors");
    if let Some(Commands::Serve(serve)) = cli.command {
        assert!(serve.cors);
    } else {
        panic!("Expected Serve command");
    }
}

#[test]
fn test_serve_command_cors_origins() {
    let cli = Cli::try_parse_from([
        "cortex",
        "serve",
        "--cors-origin",
        "http://localhost:3000",
        "--cors-origin",
        "https://example.com",
    ])
    .expect("should parse serve with cors origins");
    if let Some(Commands::Serve(serve)) = cli.command {
        assert_eq!(
            serve.cors_origins,
            vec!["http://localhost:3000", "https://example.com"]
        );
    } else {
        panic!("Expected Serve command");
    }
}

#[test]
fn test_serve_command_mdns() {
    let cli =
        Cli::try_parse_from(["cortex", "serve", "--mdns"]).expect("should parse serve --mdns");
    if let Some(Commands::Serve(serve)) = cli.command {
        assert!(serve.mdns);
    } else {
        panic!("Expected Serve command");
    }
}

// ==========================================================================
// InitCommand tests
// ==========================================================================

#[test]
fn test_init_command_default() {
    let cli = Cli::try_parse_from(["cortex", "init"]).expect("should parse init");
    if let Some(Commands::Init(init)) = cli.command {
        assert!(!init.force);
        assert!(!init.yes);
    } else {
        panic!("Expected Init command");
    }
}

#[test]
fn test_init_command_force() {
    let cli = Cli::try_parse_from(["cortex", "init", "-f"]).expect("should parse init -f");
    if let Some(Commands::Init(init)) = cli.command {
        assert!(init.force);
    } else {
        panic!("Expected Init command");
    }
}

#[test]
fn test_init_command_yes() {
    let cli = Cli::try_parse_from(["cortex", "init", "-y"]).expect("should parse init -y");
    if let Some(Commands::Init(init)) = cli.command {
        assert!(init.yes);
    } else {
        panic!("Expected Init command");
    }
}

// ==========================================================================
// CompletionCommand tests
// ==========================================================================

#[test]
fn test_completion_command_default() {
    let cli = Cli::try_parse_from(["cortex", "completion"]).expect("should parse completion");
    if let Some(Commands::Completion(completion)) = cli.command {
        assert!(completion.shell.is_none());
        assert!(!completion.install);
    } else {
        panic!("Expected Completion command");
    }
}

#[test]
fn test_completion_command_bash() {
    let cli = Cli::try_parse_from(["cortex", "completion", "bash"])
        .expect("should parse completion bash");
    if let Some(Commands::Completion(completion)) = cli.command {
        assert_eq!(completion.shell, Some(clap_complete::Shell::Bash));
    } else {
        panic!("Expected Completion command");
    }
}

#[test]
fn test_completion_command_zsh() {
    let cli =
        Cli::try_parse_from(["cortex", "completion", "zsh"]).expect("should parse completion zsh");
    if let Some(Commands::Completion(completion)) = cli.command {
        assert_eq!(completion.shell, Some(clap_complete::Shell::Zsh));
    } else {
        panic!("Expected Completion command");
    }
}

#[test]
fn test_completion_command_install() {
    let cli = Cli::try_parse_from(["cortex", "completion", "--install"])
        .expect("should parse completion --install");
    if let Some(Commands::Completion(completion)) = cli.command {
        assert!(completion.install);
    } else {
        panic!("Expected Completion command");
    }
}

// ==========================================================================
// HistoryCommand tests
// ==========================================================================

#[test]
fn test_history_command_default() {
    let cli = Cli::try_parse_from(["cortex", "history"]).expect("should parse history");
    if let Some(Commands::History(history)) = cli.command {
        assert!(history.action.is_none());
        assert_eq!(history.limit, 20);
        assert!(!history.all);
        assert!(!history.json);
    } else {
        panic!("Expected History command");
    }
}

#[test]
fn test_history_command_with_limit() {
    let cli =
        Cli::try_parse_from(["cortex", "history", "-n", "50"]).expect("should parse history -n");
    if let Some(Commands::History(history)) = cli.command {
        assert_eq!(history.limit, 50);
    } else {
        panic!("Expected History command");
    }
}

#[test]
fn test_history_search_subcommand() {
    let cli = Cli::try_parse_from(["cortex", "history", "search", "fix"])
        .expect("should parse history search");
    if let Some(Commands::History(history)) = cli.command {
        if let Some(HistorySubcommand::Search(search)) = history.action {
            assert_eq!(search.pattern, "fix");
            assert_eq!(search.limit, 20);
            assert!(!search.json);
        } else {
            panic!("Expected Search subcommand");
        }
    } else {
        panic!("Expected History command");
    }
}

#[test]
fn test_history_clear_subcommand() {
    let cli =
        Cli::try_parse_from(["cortex", "history", "clear"]).expect("should parse history clear");
    if let Some(Commands::History(history)) = cli.command {
        if let Some(HistorySubcommand::Clear(clear)) = history.action {
            assert!(!clear.yes);
        } else {
            panic!("Expected Clear subcommand");
        }
    } else {
        panic!("Expected History command");
    }
}

#[test]
fn test_history_clear_yes() {
    let cli = Cli::try_parse_from(["cortex", "history", "clear", "-y"])
        .expect("should parse history clear -y");
    if let Some(Commands::History(history)) = cli.command {
        if let Some(HistorySubcommand::Clear(clear)) = history.action {
            assert!(clear.yes);
        } else {
            panic!("Expected Clear subcommand");
        }
    } else {
        panic!("Expected History command");
    }
}

// ==========================================================================
// ServersCommand tests
// ==========================================================================

#[test]
fn test_servers_command_default() {
    let cli = Cli::try_parse_from(["cortex", "servers"]).expect("should parse servers");
    if let Some(Commands::Servers(servers)) = cli.command {
        assert!(servers.action.is_none());
        assert_eq!(servers.timeout, 3);
        assert!(!servers.json);
    } else {
        panic!("Expected Servers command");
    }
}

#[test]
fn test_servers_command_with_timeout() {
    let cli = Cli::try_parse_from(["cortex", "servers", "--timeout", "10"])
        .expect("should parse servers --timeout");
    if let Some(Commands::Servers(servers)) = cli.command {
        assert_eq!(servers.timeout, 10);
    } else {
        panic!("Expected Servers command");
    }
}

#[test]
fn test_servers_refresh_subcommand() {
    let cli = Cli::try_parse_from(["cortex", "servers", "refresh"])
        .expect("should parse servers refresh");
    if let Some(Commands::Servers(servers)) = cli.command {
        if let Some(ServersSubcommand::Refresh(refresh)) = servers.action {
            assert_eq!(refresh.timeout, 5);
            assert!(!refresh.json);
        } else {
            panic!("Expected Refresh subcommand");
        }
    } else {
        panic!("Expected Servers command");
    }
}

// ==========================================================================
// get_long_version tests
// ==========================================================================

#[test]
fn test_get_long_version() {
    let version = get_long_version();
    assert!(!version.is_empty());
    // Version should contain the package version
    assert!(
        version.contains(env!("CARGO_PKG_VERSION")),
        "Version should contain CARGO_PKG_VERSION"
    );
}

// ==========================================================================
// Conflict tests
// ==========================================================================

#[test]
fn test_dangerously_bypass_conflicts_with_approval_policy() {
    let result = Cli::try_parse_from([
        "cortex",
        "--dangerously-bypass-approvals-and-sandbox",
        "--ask-for-approval",
        "always",
    ]);
    assert!(
        result.is_err(),
        "Should fail when dangerous flag conflicts with approval policy"
    );
}

#[test]
fn test_dangerously_bypass_conflicts_with_full_auto() {
    let result = Cli::try_parse_from([
        "cortex",
        "--dangerously-bypass-approvals-and-sandbox",
        "--full-auto",
    ]);
    assert!(
        result.is_err(),
        "Should fail when dangerous flag conflicts with full-auto"
    );
}

#[test]
fn test_resume_session_id_conflicts_with_last() {
    let result = Cli::try_parse_from(["cortex", "resume", "abc123", "--last"]);
    assert!(
        result.is_err(),
        "Should fail when session_id conflicts with --last"
    );
}

#[test]
fn test_resume_pick_conflicts_with_session_id() {
    let result = Cli::try_parse_from(["cortex", "resume", "abc123", "--pick"]);
    assert!(
        result.is_err(),
        "Should fail when --pick conflicts with session_id"
    );
}

#[test]
fn test_resume_pick_conflicts_with_last() {
    let result = Cli::try_parse_from(["cortex", "resume", "--pick", "--last"]);
    assert!(
        result.is_err(),
        "Should fail when --pick conflicts with --last"
    );
}

#[test]
fn test_login_token_conflicts_with_api_key() {
    let result = Cli::try_parse_from(["cortex", "login", "--token", "xyz", "--with-api-key"]);
    assert!(
        result.is_err(),
        "Should fail when --token conflicts with --with-api-key"
    );
}

#[test]
fn test_serve_mdns_conflicts_with_no_mdns() {
    let result = Cli::try_parse_from(["cortex", "serve", "--mdns", "--no-mdns"]);
    assert!(
        result.is_err(),
        "Should fail when --mdns conflicts with --no-mdns"
    );
}

// ==========================================================================
// Alias tests
// ==========================================================================

#[test]
fn test_run_alias_r() {
    let cli = Cli::try_parse_from(["cortex", "r"]).expect("should parse 'r' alias for run");
    assert!(matches!(cli.command, Some(Commands::Run(_))));
}

#[test]
fn test_exec_alias_e() {
    let cli = Cli::try_parse_from(["cortex", "e"]).expect("should parse 'e' alias for exec");
    assert!(matches!(cli.command, Some(Commands::Exec(_))));
}

#[test]
fn test_github_alias_gh() {
    // GitHub command requires a subcommand, so test with "status"
    let cli = Cli::try_parse_from(["cortex", "gh", "status"])
        .expect("should parse 'gh status' alias for github");
    assert!(matches!(cli.command, Some(Commands::Github(_))));
}

#[test]
fn test_compact_alias_gc() {
    let cli = Cli::try_parse_from(["cortex", "gc"]).expect("should parse 'gc' alias for compact");
    assert!(matches!(cli.command, Some(Commands::Compact(_))));
}

#[test]
fn test_compact_alias_cleanup() {
    let cli = Cli::try_parse_from(["cortex", "cleanup"])
        .expect("should parse 'cleanup' alias for compact");
    assert!(matches!(cli.command, Some(Commands::Compact(_))));
}

#[test]
fn test_feedback_alias_report() {
    let cli = Cli::try_parse_from(["cortex", "report"])
        .expect("should parse 'report' alias for feedback");
    assert!(matches!(cli.command, Some(Commands::Feedback(_))));
}

#[test]
fn test_lock_alias_protect() {
    let cli =
        Cli::try_parse_from(["cortex", "protect"]).expect("should parse 'protect' alias for lock");
    assert!(matches!(cli.command, Some(Commands::Lock(_))));
}

// ==========================================================================
// FeaturesCommand tests
// ==========================================================================

#[test]
fn test_features_list_subcommand() {
    let cli =
        Cli::try_parse_from(["cortex", "features", "list"]).expect("should parse features list");
    if let Some(Commands::Features(features)) = cli.command {
        assert!(matches!(features.sub, FeaturesSubcommand::List));
    } else {
        panic!("Expected Features command");
    }
}
