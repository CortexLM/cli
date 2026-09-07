use super::*;
use runtime::{HookType, PluginContext, PluginManager};
use serde_json::{Value, json};

pub(super) async fn run(args: PluginRunArgs) -> Result<()> {
    let result = invoke(&args).await;
    if args.json {
        let report = match &result {
            Ok(value) => json!({"success":true,"result":value}),
            Err(error) => json!({"success":false,"error":error.to_string()}),
        };
        println!("{}", serde_json::to_string(&report)?);
    } else if let Ok(value) = &result {
        println!("{}", serde_json::to_string_pretty(value)?);
    }
    result.map(|_| ())
}

async fn invoke(args: &PluginRunArgs) -> Result<Value> {
    let directory = runtime::package::destination(&plugins_dir()?, &args.name)?;
    let manager = PluginManager::default_manager().await?;
    manager.load_from_path(&directory).await?;
    manager.init_all().await?;
    let context = PluginContext::new(std::env::current_dir()?)
        .with_session(format!("plugin-cli-{}", std::process::id()))
        .with_extra(
            "call_id",
            json!(format!("plugin-call-{}", std::process::id())),
        );
    let mut notifications = manager.take_notifications().await;
    let operation = async {
        let started = manager
            .dispatch_hook(
                HookType::SessionStart,
                json!({"source":"plugin.run"}),
                &context,
            )
            .await?;
        notifications.extend(started.notifications);
        if started.denied {
            bail!(
                "{}",
                started
                    .reason
                    .unwrap_or("Plugin vetoed session startup".into())
            );
        }
        let data = if let Some(tool) = &args.tool {
            let input: Value = serde_json::from_str(&args.input)?;
            let before = manager
                .dispatch_hook(
                    HookType::ToolExecuteBefore,
                    json!({"tool":tool,"args":input}),
                    &context,
                )
                .await?;
            notifications.extend(before.notifications);
            if before.denied {
                bail!(
                    "{}",
                    before
                        .reason
                        .unwrap_or("Plugin vetoed tool execution".into())
                );
            }
            // This is an explicit user invocation of an already-trusted plugin.
            // Automated callers must apply their own approval gate AFTER these hooks.
            if before.input["tool"] != *tool {
                bail!("Hooks cannot change tool identity");
            }
            let executed = manager
                .execute_tool(&args.name, tool, before.input["args"].clone(), &context)
                .await;
            let after = manager
                .dispatch_hook(
                    HookType::ToolExecuteAfter,
                    json!({"tool":tool,"success":executed.is_ok()}),
                    &context,
                )
                .await?;
            notifications.extend(after.notifications);
            let result = executed?;
            notifications.extend(result.notifications);
            result.data
        } else {
            let command = args
                .command
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("Missing plugin command"))?;
            let result = manager
                .execute_command(command, args.args.clone(), &context)
                .await?;
            serde_json::to_value(result)?
        };
        Ok::<_, anyhow::Error>(data)
    }
    .await;
    let ended = manager
        .dispatch_hook(
            HookType::SessionEnd,
            json!({"success":operation.is_ok()}),
            &context,
        )
        .await;
    let shutdown = manager.shutdown_all().await;
    notifications.extend(manager.take_notifications().await);
    let data = operation?;
    notifications.extend(ended?.notifications);
    shutdown?;
    Ok(json!({"data":data,"notifications":notifications}))
}
