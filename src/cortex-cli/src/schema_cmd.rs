//! `cortex schema` — print the shipped JSON Schemas for headless result documents.
//!
//! CI consumers pin the shape of `cortex run --format json` and
//! `cortex exec --output-format json`. This command prints the schema those
//! documents are validated against, so a pipeline can check its parser without
//! running a turn.

use anyhow::{Result, bail};
use clap::{Args, Subcommand};

use crate::schema::{EXEC_RESULT_SCHEMA, RUN_RESULT_SCHEMA, SCHEMA_NAMES, schema_document};

/// Schema CLI command.
#[derive(Debug, Args)]
pub struct SchemaCli {
    #[command(subcommand)]
    pub command: Option<SchemaSubcommand>,
}

/// Schema subcommands.
#[derive(Debug, Subcommand)]
pub enum SchemaSubcommand {
    /// Print a shipped schema as JSON.
    #[command(visible_alias = "show")]
    Print(SchemaPrintArgs),

    /// List the shipped schema names.
    #[command(visible_alias = "ls")]
    List,
}

/// Arguments for `cortex schema print`.
#[derive(Debug, Args)]
pub struct SchemaPrintArgs {
    /// Schema name to print.
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
}

impl SchemaCli {
    /// Run the schema command.
    pub fn run(self) -> Result<()> {
        match self.command {
            None | Some(SchemaSubcommand::List) => {
                for name in SCHEMA_NAMES {
                    println!("{name}");
                }
                Ok(())
            }
            Some(SchemaSubcommand::Print(args)) => {
                let name = args.name.as_deref().unwrap_or(RUN_RESULT_SCHEMA);
                let Some(schema) = schema_document(name) else {
                    bail!(
                        "Unknown schema `{name}`. Shipped schemas: {}.",
                        SCHEMA_NAMES.join(", ")
                    );
                };
                println!("{}", serde_json::to_string_pretty(&schema)?);
                Ok(())
            }
        }
    }
}

/// Names accepted by `cortex schema print`, for help text and tests.
pub const SHIPPED_SCHEMA_HINT: &str = "run-result, exec-result";

/// True when `name` is a shipped schema.
pub fn is_shipped_schema(name: &str) -> bool {
    name == RUN_RESULT_SCHEMA || name == EXEC_RESULT_SCHEMA
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_schema_is_listed_and_printable() {
        assert_eq!(SCHEMA_NAMES.len(), 2);
        for name in SCHEMA_NAMES {
            assert!(is_shipped_schema(name), "{name}");
            assert!(schema_document(name).is_some(), "{name}");
        }
        assert_eq!(SHIPPED_SCHEMA_HINT, "run-result, exec-result");
    }

    #[test]
    fn print_defaults_to_the_run_result_schema() {
        let cli = SchemaCli {
            command: Some(SchemaSubcommand::Print(SchemaPrintArgs { name: None })),
        };
        cli.run().expect("default schema");
    }

    #[test]
    fn unknown_schema_names_are_refused() {
        let cli = SchemaCli {
            command: Some(SchemaSubcommand::Print(SchemaPrintArgs {
                name: Some("nope".into()),
            })),
        };
        let error = cli.run().expect_err("unknown");
        assert!(error.to_string().contains("Unknown schema"), "{error}");
    }

    #[test]
    fn list_prints_every_shipped_name() {
        SchemaCli {
            command: Some(SchemaSubcommand::List),
        }
        .run()
        .expect("list");
        SchemaCli { command: None }.run().expect("bare");
    }
}
