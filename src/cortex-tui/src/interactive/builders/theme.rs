//! Builder for theme selection.

use crate::interactive::state::{InteractiveAction, InteractiveItem, InteractiveState};

struct ThemeDef {
    id: &'static str,
    label: &'static str,
    description: &'static str,
}

const THEMES: &[ThemeDef] = &[
    ThemeDef {
        id: "dark",
        label: "Cortex Night",
        description: "Default inky chrome · banner green on focus only",
    },
    ThemeDef {
        id: "light",
        label: "Cortex Day",
        description: "Light chrome for bright rooms",
    },
    ThemeDef {
        id: "ocean_dark",
        label: "Ocean Dark",
        description: "Deep blue and cyan accents",
    },
    ThemeDef {
        id: "monokai",
        label: "Monokai",
        description: "Classic code-editor colors",
    },
];

fn canonical_theme_id(current: Option<&str>) -> &'static str {
    match current.unwrap_or("dark") {
        "dark" | "cortex-night" | "cortex_night" => "dark",
        "light" | "cortex-day" | "cortex_day" => "light",
        "ocean_dark" | "ocean" => "ocean_dark",
        "monokai" => "monokai",
        _ => "dark",
    }
}

/// Build an interactive state for theme selection.
pub fn build_theme_selector(current: Option<&str>) -> InteractiveState {
    let current_theme = canonical_theme_id(current);

    let items: Vec<InteractiveItem> = THEMES
        .iter()
        .map(|t| {
            let is_current = t.id == current_theme;
            InteractiveItem::new(t.id, t.label)
                .with_description(t.description)
                .with_current(is_current)
                .with_icon(if is_current { '>' } else { ' ' })
        })
        .collect();

    InteractiveState::new("Theme", items, InteractiveAction::Custom("theme".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_theme_selector() {
        let state = build_theme_selector(Some("ocean"));
        assert!(!state.items.is_empty());
        assert_eq!(state.title, "Theme");

        let current = state.items.iter().find(|i| i.is_current);
        assert!(current.is_some());
        assert_eq!(current.unwrap().id, "ocean_dark");
        assert_eq!(state.items[0].label, "Cortex Night");
        assert_eq!(state.items[1].label, "Cortex Day");
    }
}
