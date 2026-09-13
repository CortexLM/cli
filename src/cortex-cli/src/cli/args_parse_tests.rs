use super::*;
use clap::Parser;

// ==========================================================================
// LogLevel tests
// ==========================================================================

#[test]
fn test_log_level_default() {
    let default = LogLevel::default();
    assert_eq!(default, LogLevel::Info);
}

#[test]
fn test_log_level_as_filter_str() {
    assert_eq!(LogLevel::Error.as_filter_str(), "error");
    assert_eq!(LogLevel::Warn.as_filter_str(), "warn");
    assert_eq!(LogLevel::Info.as_filter_str(), "info");
    assert_eq!(LogLevel::Debug.as_filter_str(), "debug");
    assert_eq!(LogLevel::Trace.as_filter_str(), "trace");
}

#[test]
fn test_log_level_from_str_loose_valid() {
    assert_eq!(LogLevel::from_str_loose("error"), Some(LogLevel::Error));
    assert_eq!(LogLevel::from_str_loose("warn"), Some(LogLevel::Warn));
    assert_eq!(LogLevel::from_str_loose("warning"), Some(LogLevel::Warn));
    assert_eq!(LogLevel::from_str_loose("info"), Some(LogLevel::Info));
    assert_eq!(LogLevel::from_str_loose("debug"), Some(LogLevel::Debug));
    assert_eq!(LogLevel::from_str_loose("trace"), Some(LogLevel::Trace));
}

#[test]
fn test_log_level_from_str_loose_case_insensitive() {
    assert_eq!(LogLevel::from_str_loose("ERROR"), Some(LogLevel::Error));
    assert_eq!(LogLevel::from_str_loose("WARN"), Some(LogLevel::Warn));
    assert_eq!(LogLevel::from_str_loose("WARNING"), Some(LogLevel::Warn));
    assert_eq!(LogLevel::from_str_loose("INFO"), Some(LogLevel::Info));
    assert_eq!(LogLevel::from_str_loose("Debug"), Some(LogLevel::Debug));
    assert_eq!(LogLevel::from_str_loose("TrAcE"), Some(LogLevel::Trace));
}

#[test]
fn test_log_level_from_str_loose_invalid() {
    assert_eq!(LogLevel::from_str_loose("invalid"), None);
    assert_eq!(LogLevel::from_str_loose(""), None);
    assert_eq!(LogLevel::from_str_loose("err"), None);
    assert_eq!(LogLevel::from_str_loose("verbose"), None);
}

#[test]
fn test_log_level_equality() {
    assert_eq!(LogLevel::Error, LogLevel::Error);
    assert_ne!(LogLevel::Error, LogLevel::Warn);
    assert_ne!(LogLevel::Info, LogLevel::Debug);
}

#[test]
fn test_log_level_clone() {
    let level = LogLevel::Debug;
    let cloned = level;
    assert_eq!(level, cloned);
}

// ==========================================================================
// ColorMode tests
// ==========================================================================

#[test]
fn test_color_mode_default() {
    let default = ColorMode::default();
    assert_eq!(default, ColorMode::Auto);
}

#[test]
fn test_color_mode_equality() {
    assert_eq!(ColorMode::Auto, ColorMode::Auto);
    assert_eq!(ColorMode::Always, ColorMode::Always);
    assert_eq!(ColorMode::Never, ColorMode::Never);
    assert_ne!(ColorMode::Auto, ColorMode::Always);
    assert_ne!(ColorMode::Always, ColorMode::Never);
}

#[test]
fn test_color_mode_clone() {
    let mode = ColorMode::Always;
    let cloned = mode;
    assert_eq!(mode, cloned);
}

// ==========================================================================
// InteractiveArgs tests
// ==========================================================================

#[test]
fn test_interactive_args_default() {
    let args = InteractiveArgs::default();
    assert!(args.model.is_none());
    assert!(!args.oss);
    assert!(args.config_profile.is_none());
    assert!(args.sandbox_mode.is_none());
    assert!(args.approval_policy.is_none());
    assert!(!args.full_auto);
    assert!(!args.dangerously_bypass_approvals_and_sandbox);
    assert!(args.cwd.is_none());
    assert!(args.add_dir.is_empty());
    assert!(args.images.is_empty());
    assert!(!args.web_search);
    assert!(!args.alternate_screen);
    assert!(!args.no_alternate_screen);
    assert_eq!(args.log_level, LogLevel::Info);
    assert!(!args.debug);
    assert!(args.prompt.is_empty());
}

// ==========================================================================
// Cli parsing tests
// ==========================================================================

#[test]
fn test_cli_no_args() {
    let cli = Cli::try_parse_from(["cortex"]).expect("should parse with no args");
    assert!(cli.command.is_none());
    assert!(!cli.verbose);
    assert!(!cli.trace);
    assert_eq!(cli.color, ColorMode::Auto);
}

#[test]
fn test_cli_verbose_flag() {
    let cli = Cli::try_parse_from(["cortex", "--verbose"]).expect("should parse --verbose");
    assert!(cli.verbose);
}

#[test]
fn test_cli_verbose_short_flag() {
    let cli = Cli::try_parse_from(["cortex", "-v"]).expect("should parse -v");
    assert!(cli.verbose);
}

#[test]
fn test_cli_trace_flag() {
    let cli = Cli::try_parse_from(["cortex", "--trace"]).expect("should parse --trace");
    assert!(cli.trace);
}

#[test]
fn test_cli_color_always() {
    let cli =
        Cli::try_parse_from(["cortex", "--color", "always"]).expect("should parse --color always");
    assert_eq!(cli.color, ColorMode::Always);
}

#[test]
fn test_cli_color_never() {
    let cli =
        Cli::try_parse_from(["cortex", "--color", "never"]).expect("should parse --color never");
    assert_eq!(cli.color, ColorMode::Never);
}

#[test]
fn test_cli_color_auto() {
    let cli =
        Cli::try_parse_from(["cortex", "--color", "auto"]).expect("should parse --color auto");
    assert_eq!(cli.color, ColorMode::Auto);
}

#[test]
fn test_cli_model_short() {
    let cli = Cli::try_parse_from(["cortex", "-m", "gpt-4o"]).expect("should parse -m");
    assert_eq!(cli.interactive.model, Some("gpt-4o".to_string()));
}

#[test]
fn test_cli_model_long() {
    let cli = Cli::try_parse_from(["cortex", "--model", "claude-sonnet-4-20250514"])
        .expect("should parse --model");
    assert_eq!(
        cli.interactive.model,
        Some("claude-sonnet-4-20250514".to_string())
    );
}

#[test]
fn test_cli_oss_flag() {
    let cli = Cli::try_parse_from(["cortex", "--oss"]).expect("should parse --oss");
    assert!(cli.interactive.oss);
}

#[test]
fn test_cli_profile_short() {
    let cli = Cli::try_parse_from(["cortex", "-p", "work"]).expect("should parse -p");
    assert_eq!(cli.interactive.config_profile, Some("work".to_string()));
}

#[test]
fn test_cli_profile_long() {
    let cli =
        Cli::try_parse_from(["cortex", "--profile", "production"]).expect("should parse --profile");
    assert_eq!(
        cli.interactive.config_profile,
        Some("production".to_string())
    );
}

#[test]
fn test_cli_sandbox_mode() {
    let cli =
        Cli::try_parse_from(["cortex", "--sandbox", "strict"]).expect("should parse --sandbox");
    assert_eq!(cli.interactive.sandbox_mode, Some("strict".to_string()));
}

#[test]
fn test_cli_approval_policy() {
    let cli = Cli::try_parse_from(["cortex", "--ask-for-approval", "always"])
        .expect("should parse --ask-for-approval");
    assert_eq!(cli.interactive.approval_policy, Some("always".to_string()));
}

#[test]
fn test_cli_approval_policy_short() {
    let cli = Cli::try_parse_from(["cortex", "-a", "never"]).expect("should parse -a");
    assert_eq!(cli.interactive.approval_policy, Some("never".to_string()));
}

#[test]
fn test_cli_full_auto() {
    let cli = Cli::try_parse_from(["cortex", "--full-auto"]).expect("should parse --full-auto");
    assert!(cli.interactive.full_auto);
}

#[test]
fn test_cli_dangerously_bypass() {
    let cli = Cli::try_parse_from(["cortex", "--dangerously-bypass-approvals-and-sandbox"])
        .expect("should parse dangerous flag");
    assert!(cli.interactive.dangerously_bypass_approvals_and_sandbox);
}

#[test]
fn test_cli_dangerously_bypass_yolo_alias() {
    let cli = Cli::try_parse_from(["cortex", "--yolo"]).expect("should parse --yolo alias");
    assert!(cli.interactive.dangerously_bypass_approvals_and_sandbox);
}

#[test]
fn test_cli_cwd_short() {
    let cli = Cli::try_parse_from(["cortex", "-C", "/workspace"]).expect("should parse -C");
    assert_eq!(cli.interactive.cwd, Some(PathBuf::from("/workspace")));
}

#[test]
fn test_cli_cwd_long() {
    let cli = Cli::try_parse_from(["cortex", "--cd", "/tmp/project"]).expect("should parse --cd");
    assert_eq!(cli.interactive.cwd, Some(PathBuf::from("/tmp/project")));
}

#[test]
fn test_cli_add_dir() {
    let cli =
        Cli::try_parse_from(["cortex", "--add-dir", "/extra/dir"]).expect("should parse --add-dir");
    assert_eq!(cli.interactive.add_dir, vec![PathBuf::from("/extra/dir")]);
}

#[test]
fn test_cli_add_dir_multiple() {
    let cli = Cli::try_parse_from(["cortex", "--add-dir", "/dir1", "--add-dir", "/dir2"])
        .expect("should parse multiple --add-dir");
    assert_eq!(
        cli.interactive.add_dir,
        vec![PathBuf::from("/dir1"), PathBuf::from("/dir2")]
    );
}

#[test]
fn test_cli_image() {
    let cli =
        Cli::try_parse_from(["cortex", "--image", "screenshot.png"]).expect("should parse --image");
    assert_eq!(
        cli.interactive.images,
        vec![PathBuf::from("screenshot.png")]
    );
}

#[test]
fn test_cli_image_short() {
    let cli = Cli::try_parse_from(["cortex", "-i", "photo.jpg"]).expect("should parse -i");
    assert_eq!(cli.interactive.images, vec![PathBuf::from("photo.jpg")]);
}

#[test]
fn test_cli_image_comma_separated() {
    let cli = Cli::try_parse_from(["cortex", "--image", "a.png,b.jpg,c.gif"])
        .expect("should parse comma-separated images");
    assert_eq!(
        cli.interactive.images,
        vec![
            PathBuf::from("a.png"),
            PathBuf::from("b.jpg"),
            PathBuf::from("c.gif")
        ]
    );
}

#[test]
fn test_cli_web_search() {
    let cli = Cli::try_parse_from(["cortex", "--search"]).expect("should parse --search");
    assert!(cli.interactive.web_search);
}

#[test]
fn test_cli_log_level() {
    let cli =
        Cli::try_parse_from(["cortex", "--log-level", "debug"]).expect("should parse --log-level");
    assert_eq!(cli.interactive.log_level, LogLevel::Debug);
}

#[test]
fn test_cli_log_level_short() {
    let cli = Cli::try_parse_from(["cortex", "-L", "trace"]).expect("should parse -L");
    assert_eq!(cli.interactive.log_level, LogLevel::Trace);
}

#[test]
fn test_cli_debug_flag() {
    let cli = Cli::try_parse_from(["cortex", "--debug"]).expect("should parse --debug");
    assert!(cli.interactive.debug);
}

#[test]
fn test_cli_alternate_screen_flag_defaults_unset() {
    let cli = Cli::try_parse_from(["cortex"]).expect("should parse");
    assert!(
        !cli.interactive.alternate_screen,
        "the flag is unset unless passed; config default is always"
    );
    assert!(
        !cli.interactive.no_alternate_screen,
        "inline opt-out is unset unless --no-alternate-screen is passed"
    );
    let cli = Cli::try_parse_from(["cortex", "--alternate-screen"])
        .expect("should parse --alternate-screen");
    assert!(cli.interactive.alternate_screen);
    assert!(!cli.interactive.no_alternate_screen);
    let cli = Cli::try_parse_from(["cortex", "--no-alternate-screen"])
        .expect("should parse --no-alternate-screen");
    assert!(cli.interactive.no_alternate_screen);
    assert!(!cli.interactive.alternate_screen);
}

#[test]
fn test_cli_alternate_screen_conflicts_with_no_alternate_screen() {
    let result = Cli::try_parse_from(["cortex", "--alternate-screen", "--no-alternate-screen"]);
    match result {
        Err(err) => {
            let rendered = err.to_string();
            assert!(
                rendered.contains("cannot be used with") || rendered.contains("conflict"),
                "unexpected clap error: {rendered}"
            );
        }
        Ok(_) => panic!("--alternate-screen and --no-alternate-screen must conflict"),
    }
}

#[test]
fn test_cli_prompt_trailing() {
    let cli = Cli::try_parse_from(["cortex", "write", "a", "unit", "test"])
        .expect("should parse trailing prompt");
    assert_eq!(
        cli.interactive.prompt,
        vec!["write", "a", "unit", "test"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_cli_prompt_with_hyphens() {
    let cli = Cli::try_parse_from(["cortex", "create", "--", "test", "--with", "options"])
        .expect("should parse prompt with hyphens");
    assert!(!cli.interactive.prompt.is_empty());
}

#[test]
fn test_cli_harness_flags_and_attach_jobs() {
    let cli = Cli::try_parse_from([
        "cortex",
        "--plugin-dir",
        "/tmp/plugins",
        "--bash-edit-diff",
        "--worktree",
    ])
    .expect("should parse harness flags");
    assert_eq!(
        cli.interactive.plugin_dir,
        vec![std::path::PathBuf::from("/tmp/plugins")]
    );
    assert!(cli.interactive.bash_edit_diff);
    assert_eq!(
        cli.interactive.worktree.as_deref(),
        Some(std::path::Path::new("auto"))
    );

    let cli = Cli::try_parse_from(["cortex", "attach", "sess-9"]).expect("attach");
    assert!(matches!(cli.command, Some(Commands::Attach(_))));
    let cli = Cli::try_parse_from(["cortex", "jobs", "list"]).expect("jobs");
    assert!(matches!(cli.command, Some(Commands::Jobs(_))));
}

// ==========================================================================
// Subcommand tests
// ==========================================================================

#[test]
fn test_cli_run_subcommand() {
    let cli = Cli::try_parse_from(["cortex", "run"]).expect("should parse run subcommand");
    assert!(matches!(cli.command, Some(Commands::Run(_))));
}

#[test]
fn test_cli_exec_subcommand() {
    let cli = Cli::try_parse_from(["cortex", "exec"]).expect("should parse exec subcommand");
    assert!(matches!(cli.command, Some(Commands::Exec(_))));
}

#[test]
fn test_cli_login_subcommand() {
    let cli = Cli::try_parse_from(["cortex", "login"]).expect("should parse login subcommand");
    assert!(matches!(cli.command, Some(Commands::Login(_))));
}

#[test]
fn test_cli_logout_subcommand() {
    let cli = Cli::try_parse_from(["cortex", "logout"]).expect("should parse logout subcommand");
    assert!(matches!(cli.command, Some(Commands::Logout(_))));
}

#[test]
fn test_cli_whoami_subcommand() {
    let cli = Cli::try_parse_from(["cortex", "whoami"]).expect("should parse whoami subcommand");
    assert!(matches!(cli.command, Some(Commands::Whoami)));
}

#[test]
fn test_cli_config_subcommand() {
    let cli = Cli::try_parse_from(["cortex", "config"]).expect("should parse config subcommand");
    assert!(matches!(cli.command, Some(Commands::Config(_))));
}

#[test]
fn test_cli_models_subcommand() {
    let cli = Cli::try_parse_from(["cortex", "models"]).expect("should parse models subcommand");
    assert!(matches!(cli.command, Some(Commands::Models(_))));
}

#[test]
fn test_cli_sessions_subcommand() {
    let cli =
        Cli::try_parse_from(["cortex", "sessions"]).expect("should parse sessions subcommand");
    assert!(matches!(cli.command, Some(Commands::Sessions(_))));
}

#[test]
fn test_cli_resume_subcommand() {
    let cli = Cli::try_parse_from(["cortex", "resume"]).expect("should parse resume subcommand");
    assert!(matches!(cli.command, Some(Commands::Resume(_))));
}

#[test]
fn test_cli_resume_with_session_id() {
    let cli = Cli::try_parse_from(["cortex", "resume", "abc123"])
        .expect("should parse resume with session id");
    if let Some(Commands::Resume(resume)) = cli.command {
        assert_eq!(resume.session_id, Some("abc123".to_string()));
    } else {
        panic!("Expected Resume command");
    }
}

#[test]
fn test_cli_resume_last_flag() {
    let cli =
        Cli::try_parse_from(["cortex", "resume", "--last"]).expect("should parse resume --last");
    if let Some(Commands::Resume(resume)) = cli.command {
        assert!(resume.last);
    } else {
        panic!("Expected Resume command");
    }
}

#[test]
fn test_cli_upgrade_subcommand() {
    let cli = Cli::try_parse_from(["cortex", "upgrade"]).expect("should parse upgrade subcommand");
    assert!(matches!(cli.command, Some(Commands::Upgrade(_))));
}

#[test]
fn test_cli_agent_subcommand() {
    // Agent command requires a subcommand, so test with "list"
    let cli = Cli::try_parse_from(["cortex", "agent", "list"])
        .expect("should parse agent list subcommand");
    assert!(matches!(cli.command, Some(Commands::Agent(_))));
}

// NOTE: test_cli_mcp_subcommand is skipped due to a pre-existing bug in mcp_cmd
// where "ls" is defined as both a command name and an alias, causing clap to panic.
// This is tracked as a known issue in the mcp_cmd module (types.rs:27).

#[test]
fn test_cli_acp_subcommand() {
    let cli = Cli::try_parse_from(["cortex", "acp"]).expect("should parse acp subcommand");
    assert!(matches!(cli.command, Some(Commands::Acp(_))));
}

// ==========================================================================
// LoginCommand tests
// ==========================================================================

#[test]
fn test_login_command_default() {
    let cli = Cli::try_parse_from(["cortex", "login"]).expect("should parse login");
    if let Some(Commands::Login(login)) = cli.command {
        assert!(!login.with_api_key);
        assert!(login.token.is_none());
        assert!(!login.use_device_code);
        assert!(!login.use_sso);
        assert!(login.issuer_base_url.is_none());
        assert!(login.client_id.is_none());
        assert!(login.action.is_none());
    } else {
        panic!("Expected Login command");
    }
}

#[test]
fn test_login_command_with_api_key() {
    let cli = Cli::try_parse_from(["cortex", "login", "--with-api-key"])
        .expect("should parse login --with-api-key");
    if let Some(Commands::Login(login)) = cli.command {
        assert!(login.with_api_key);
    } else {
        panic!("Expected Login command");
    }
}

#[test]
fn test_login_command_with_token() {
    let cli = Cli::try_parse_from(["cortex", "login", "--token", "mytoken123"])
        .expect("should parse login --token");
    if let Some(Commands::Login(login)) = cli.command {
        assert_eq!(login.token, Some("mytoken123".to_string()));
    } else {
        panic!("Expected Login command");
    }
}

#[test]
fn test_login_command_device_auth() {
    let cli = Cli::try_parse_from(["cortex", "login", "--device-auth"])
        .expect("should parse login --device-auth");
    if let Some(Commands::Login(login)) = cli.command {
        assert!(login.use_device_code);
    } else {
        panic!("Expected Login command");
    }
}

#[test]
fn test_login_command_sso() {
    let cli = Cli::try_parse_from(["cortex", "login", "--sso"]).expect("should parse login --sso");
    if let Some(Commands::Login(login)) = cli.command {
        assert!(login.use_sso);
    } else {
        panic!("Expected Login command");
    }
}

#[test]
fn test_login_status_subcommand() {
    let cli =
        Cli::try_parse_from(["cortex", "login", "status"]).expect("should parse login status");
    if let Some(Commands::Login(login)) = cli.command {
        assert!(matches!(login.action, Some(LoginSubcommand::Status)));
    } else {
        panic!("Expected Login command");
    }
}

// ==========================================================================
// LogoutCommand tests
// ==========================================================================

#[test]
fn test_logout_command_default() {
    let cli = Cli::try_parse_from(["cortex", "logout"]).expect("should parse logout");
    if let Some(Commands::Logout(logout)) = cli.command {
        assert!(!logout.yes);
        assert!(!logout.all);
    } else {
        panic!("Expected Logout command");
    }
}

#[test]
fn test_logout_command_yes() {
    let cli = Cli::try_parse_from(["cortex", "logout", "-y"]).expect("should parse logout -y");
    if let Some(Commands::Logout(logout)) = cli.command {
        assert!(logout.yes);
    } else {
        panic!("Expected Logout command");
    }
}

#[test]
fn test_logout_command_all() {
    let cli =
        Cli::try_parse_from(["cortex", "logout", "--all"]).expect("should parse logout --all");
    if let Some(Commands::Logout(logout)) = cli.command {
        assert!(logout.all);
    } else {
        panic!("Expected Logout command");
    }
}
