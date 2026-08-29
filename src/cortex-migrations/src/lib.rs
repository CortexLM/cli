//! Model migration warnings and deprecation notices for Cortex CLI.
//!
//! Tracks deprecated models and provides migration guidance.

pub mod deprecations;
pub mod migrations;
pub mod warnings;

pub use deprecations::{DEPRECATED_MODELS, DeprecatedModel, DeprecationInfo};
pub use migrations::{MigrationPath, get_migration_path};
pub use warnings::{ModelWarning, WarningLevel, check_model_warnings};
