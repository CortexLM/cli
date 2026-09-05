//! Builder for model selection.

use crate::interactive::state::{
    EffortLevel, InteractiveAction, InteractiveItem, InteractiveState,
};
use crate::providers::models::ModelInfo;

/// Build an interactive state for model selection.
/// Models should be passed from ProviderManager.available_models().
///
/// Effort is Low / Medium / High radios on this surface. Tab cycles them.
/// There is no separate A★ `/effort` picker.
pub fn build_model_selector(
    models: Vec<ModelInfo>,
    current_model: Option<&str>,
    current_effort: Option<&str>,
) -> InteractiveState {
    let mut items: Vec<InteractiveItem> = models
        .iter()
        .map(|model| {
            let is_current = current_model.map(|c| c == model.id).unwrap_or(false);

            let description = format_model_description(model);

            InteractiveItem::new(&model.id, &model.name)
                .with_description(description)
                .with_current(is_current)
                .with_metadata(model.id.clone())
        })
        .collect();

    // Sort: current first, then by name
    items.sort_by(|a, b| {
        if a.is_current && !b.is_current {
            std::cmp::Ordering::Less
        } else if !a.is_current && b.is_current {
            std::cmp::Ordering::Greater
        } else {
            a.label.cmp(&b.label)
        }
    });

    let title = "Select Model".to_string();

    InteractiveState::new(title, items, InteractiveAction::SetModel)
        .with_search()
        .with_max_visible(15)
        .with_effort(EffortLevel::parse(current_effort))
        .with_hints(vec![
            ("↑↓".into(), "select".into()),
            ("↵".into(), "confirm".into()),
            ("tab".into(), "effort".into()),
            ("esc".into(), "close".into()),
        ])
}

/// Format a model description showing context window and other info.
fn format_model_description(model: &ModelInfo) -> String {
    let mut parts = Vec::new();

    // Context window
    let ctx = model.context_window;
    let ctx_str = if ctx >= 1_000_000 {
        format!("{}M ctx", ctx / 1_000_000)
    } else if ctx >= 1_000 {
        format!("{}K ctx", ctx / 1_000)
    } else {
        format!("{} ctx", ctx)
    };
    parts.push(ctx_str);

    // Capabilities
    if model.vision {
        parts.push("vision".to_string());
    }
    if model.tools {
        parts.push("tools".to_string());
    }

    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_model_selector() {
        let state = build_model_selector(Vec::new(), None, None);
        // May be empty if no models configured, but should not panic
        assert_eq!(state.title, "Select Model");
        assert!(state.searchable);
        assert_eq!(state.effort, Some(EffortLevel::Medium));
        let hints = state.hints.expect("tab effort hints");
        assert!(
            hints.iter().any(|(k, a)| k == "tab" && a == "effort"),
            "{hints:?}"
        );
    }

    #[test]
    fn model_selector_honors_current_effort() {
        let state = build_model_selector(Vec::new(), None, Some("high"));
        assert_eq!(state.effort, Some(EffortLevel::High));
        assert_eq!(
            state.effort.expect("effort").radios_line(),
            "○ Low   ○ Medium   ● High"
        );
    }

    #[test]
    fn model_selector_has_no_star_effort_picker() {
        let state = build_model_selector(Vec::new(), None, Some("low"));
        let line = state.effort.expect("effort").radios_line();
        assert!(!line.contains('★') && !line.contains("A★"), "{line}");
        assert!(line.contains("Low") && line.contains("Medium") && line.contains("High"));
    }
}
