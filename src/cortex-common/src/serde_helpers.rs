//! Serde helper functions for default values.
//!
//! Provides commonly-needed serde default functions that are used across
//! multiple crates to reduce code duplication.

/// Returns `true` for serde default values.
///
/// Used when a boolean field should default to `true` in serde structs.
///
/// # Example
/// ```ignore
/// #[derive(Deserialize)]
/// struct Config {
///     #[serde(default = "default_true")]
///     enabled: bool,
/// }
/// ```
pub fn default_true() -> bool {
    true
}

/// Returns `false` for serde default values.
pub fn default_false() -> bool {
    false
}

/// Returns an empty string for serde default values.
pub fn default_empty_string() -> String {
    String::new()
}

/// Returns an empty Vec for serde default values.
pub fn default_empty_vec<T>() -> Vec<T> {
    Vec::new()
}

/// Returns a default empty HashMap for serde default values.
pub use std::collections::HashMap as StdHashMap;
pub fn default_empty_hashmap<K, V>() -> StdHashMap<K, V> {
    StdHashMap::new()
}
