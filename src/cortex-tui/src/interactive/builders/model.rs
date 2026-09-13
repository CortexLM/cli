//! Builder for model selection.

use crate::interactive::state::{
    EffortLevel, InteractiveAction, InteractiveItem, InteractiveState,
};
use crate::providers::models::ModelInfo;
use crate::ui::text_utils::{contains_foreign_brand, model_display_name};

/// Build an interactive state for model selection.
/// Models should be passed from ProviderManager.available_models().
///
/// Effort is High / Medium / Low radios on this surface. Tab jumps to them.
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
            let (label, description) = format_model_row(model, is_current);
            InteractiveItem::new(&model.id, label)
                .with_description(description)
                .with_current(is_current)
                .with_metadata(model.id.clone())
        })
        .collect();

    items.sort_by_key(|item| catalog_rank(&item.label));

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

/// Lock v2 `/model` copy. Custom / third-party ids stay Cortex-safe.
fn format_model_row(model: &ModelInfo, is_current: bool) -> (String, String) {
    let label = cortex_safe_label(&model.id, &model.name);
    let mut desc = lock_description(&model.id, &label).unwrap_or_else(|| {
        let raw = model.description.trim();
        if !raw.is_empty() && !looks_foreign(raw) {
            raw.to_string()
        } else {
            "Custom model".to_string()
        }
    });
    if is_max_label(&label) && !desc.contains("MAX") {
        desc.push_str(" · MAX");
    }
    if is_current {
        desc.push_str(" · current");
    }
    (label, desc)
}

fn cortex_safe_label(id: &str, name: &str) -> String {
    for candidate in [name, id] {
        let pretty = model_display_name(candidate);
        if !looks_foreign(candidate) && !looks_foreign(&pretty) {
            return pretty;
        }
    }
    "Custom model".to_string()
}

fn lock_description(id: &str, label: &str) -> Option<String> {
    let key = id.rsplit('/').next().unwrap_or(id);
    let text = if key.eq_ignore_ascii_case("cortex-1-mini") || label == "Cortex Mini 1" {
        Some("Fast default for everyday coding")
    } else if key.eq_ignore_ascii_case("cortex-1-max") || label == "Cortex Max 1" {
        Some("Longest context — bills by token instead of per request")
    } else if key.eq_ignore_ascii_case("cortex-1") || label == "Cortex 1" {
        Some("Deeper reasoning for hard changes")
    } else {
        None
    };
    text.map(str::to_string)
}

fn is_max_label(label: &str) -> bool {
    label.to_ascii_lowercase().contains("max")
}

fn catalog_rank(label: &str) -> u8 {
    match label {
        "Cortex Mini 1" => 0,
        "Cortex 1" => 1,
        "Cortex Max 1" => 2,
        _ => 10,
    }
}

fn looks_foreign(text: &str) -> bool {
    contains_foreign_brand(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_model_selector() {
        let state = build_model_selector(Vec::new(), None, None);
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
            "● High Effort   ○ Medium Effort   ○ Low Effort"
        );
    }

    #[test]
    fn model_selector_has_no_star_effort_picker() {
        let state = build_model_selector(Vec::new(), None, Some("low"));
        let line = state.effort.expect("effort").radios_line();
        assert!(!line.contains('★') && !line.contains("A★"), "{line}");
        assert!(line.contains("Low") && line.contains("Medium") && line.contains("High"));
    }

    #[test]
    fn lock_rows_use_catalog_copy_and_hide_foreign_brands() {
        let models = vec![
            ModelInfo::new("cortex-1-mini", "Cortex Mini 1", "cortex"),
            ModelInfo::new("cortex-1", "Cortex 1", "cortex"),
            ModelInfo::new("cortex-1-max", "Cortex Max 1", "cortex"),
            ModelInfo::new("acme/other-router", "Custom model", "custom"),
        ];
        let state = build_model_selector(models, Some("cortex-1-mini"), Some("medium"));
        let labels: Vec<_> = state.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            labels,
            ["Cortex Mini 1", "Cortex 1", "Cortex Max 1", "Custom model"]
        );
        assert!(
            state.items[0]
                .description
                .as_deref()
                .unwrap_or("")
                .contains("Fast default for everyday coding")
        );
        assert!(
            state.items[0]
                .description
                .as_deref()
                .unwrap_or("")
                .contains("current")
        );
        assert!(
            state.items[2]
                .description
                .as_deref()
                .unwrap_or("")
                .contains("Longest context")
        );
        assert!(
            state.items[2]
                .description
                .as_deref()
                .unwrap_or("")
                .contains("MAX")
        );
        let joined = state
            .items
            .iter()
            .map(|i| format!("{} {}", i.label, i.description.clone().unwrap_or_default()))
            .collect::<Vec<_>>()
            .join("\n")
            .to_ascii_lowercase();
        assert!(!joined.contains("claude"));
        assert!(!joined.contains("anthropic"));
        assert!(!joined.contains("openai"));
        assert!(!joined.contains("gpt-4"));
    }

    #[test]
    fn foreign_provider_ids_become_custom_model() {
        let models = vec![ModelInfo::new(
            "anthropic/claude-sonnet-4",
            "Claude Sonnet 4",
            "anthropic",
        )];
        let state = build_model_selector(models, None, None);
        assert_eq!(state.items[0].label, "Custom model");
        let blob = format!(
            "{} {}",
            state.items[0].label,
            state.items[0].description.clone().unwrap_or_default()
        )
        .to_ascii_lowercase();
        assert!(!blob.contains("claude"));
        assert!(!blob.contains("anthropic"));
        assert!(!blob.contains("sonnet"));
    }
}
