use clap::CommandFactory;
use serde_json::{Value, json};

fn describe(command: &clap::Command) -> Value {
    json!({
        "name": command.get_name(),
        "about": command.get_about().map(ToString::to_string),
        "arguments": command.get_arguments().map(|arg| json!({
            "id": arg.get_id().as_str(),
            "long": arg.get_long(),
            "short": arg.get_short(),
            "required": arg.is_required_set(),
            "help": arg.get_help().map(ToString::to_string),
        })).collect::<Vec<_>>(),
        "subcommands": command.get_subcommands().map(describe).collect::<Vec<_>>(),
    })
}

fn main() {
    let mut command = cortex_cli::cli::Cli::command();
    command.build();
    println!(
        "{}",
        serde_json::to_string_pretty(&describe(&command)).unwrap()
    );
}
