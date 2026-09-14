//! Builders for session pickers: mode, effort, sandbox, and skills.

use crate::interactive::state::{InteractiveAction, InteractiveItem, InteractiveState};

/// Build Agent / Plan / Ask mode picker.
pub fn build_mode_selector(current: &str) -> InteractiveState {
    let current = current.to_ascii_lowercase();
    let items = vec![
        InteractiveItem::new("agent", "Agent")
            .with_description("edits files and runs commands")
            .with_current(current == "agent"),
        InteractiveItem::new("plan", "Plan")
            .with_description("draft an approach first — no edits")
            .with_current(current == "plan"),
        InteractiveItem::new("ask", "Ask")
            .with_description("read-only answers on the codebase")
            .with_current(current == "ask"),
    ];
    InteractiveState::new("Mode", items, InteractiveAction::Custom("mode".to_string()))
}

/// Build Low / Medium / High effort radios for tests that still construct
/// a standalone effort list. Live `/effort` opens `/model` instead.
pub fn build_effort_selector(current: Option<&str>) -> InteractiveState {
    crate::interactive::builders::build_model_selector(Vec::new(), None, current)
}

/// Build sandbox on/off picker.
pub fn build_sandbox_selector(enabled: bool) -> InteractiveState {
    let items = vec![
        InteractiveItem::new("on", "On")
            .with_description("Commands run in the workspace sandbox")
            .with_current(enabled),
        InteractiveItem::new("off", "Off")
            .with_description("No sandbox — ask before leaving the workspace")
            .with_current(!enabled),
    ];
    InteractiveState::new(
        "Sandbox",
        items,
        InteractiveAction::Custom("sandbox".to_string()),
    )
}

/// Build the sandbox network allowlist picker from the committed entries.
///
/// The list is what the sandbox actually consults, so an empty list is shown as
/// blocking everything rather than as "no entries yet".
pub fn build_sandbox_allowlist(
    allowlist: &crate::sandbox_allowlist::SandboxAllowlist,
    selected: usize,
    hovered: Option<usize>,
) -> InteractiveState {
    let entries = allowlist.domains();
    let mut items: Vec<InteractiveItem> = if entries.is_empty() {
        vec![
            InteractiveItem::new("__none__", "No domains allowed")
                .with_description("Everything off this list is blocked")
                .with_disabled(true),
        ]
    } else {
        entries
            .iter()
            .map(|entry| {
                let mut item = InteractiveItem::new(entry.host.clone(), entry.host.clone());
                item = item.with_description(match entry.note.as_deref() {
                    Some(note) => note.to_string(),
                    None => "allowed".to_string(),
                });
                item
            })
            .collect()
    };
    items.push(
        InteractiveItem::new("__add__", "a Add a domain")
            .with_description("asks before it leaves the sandbox"),
    );
    let mut interactive = InteractiveState::new(
        "Sandbox · network",
        items,
        InteractiveAction::Custom("sandbox-allowlist".to_string()),
    );
    interactive.selected = selected.min(interactive.items.len().saturating_sub(1));
    interactive.hovered = hovered;
    interactive
}

/// One skill row for `/skills`.
pub struct SkillListItem {
    pub name: String,
    pub description: String,
}

/// Build the plugin marketplace picker: installed plugins, then the registry.
pub fn build_plugin_marketplace(
    installed: &[(String, String)],
    selected: usize,
    hovered: Option<usize>,
) -> InteractiveState {
    let state = crate::plugin_marketplace::PluginState {
        plugins: installed
            .iter()
            .map(|(id, version)| crate::plugin_marketplace::InstalledPlugin {
                id: id.clone(),
                version: version.clone(),
                enabled: true,
            })
            .collect(),
    };
    let items: Vec<InteractiveItem> = state
        .marketplace_rows()
        .into_iter()
        .map(|(id, label, description)| {
            let mut item = InteractiveItem::new(id, label).with_description(description);
            if item.id == "__search__" {
                item = item.with_shortcut('s');
            }
            item
        })
        .collect();
    let mut interactive = InteractiveState::new(
        "Plugin marketplace",
        items,
        InteractiveAction::Custom("plugin-marketplace".into()),
    )
    .with_banner(format!(
        "{} — signed packages only.",
        crate::plugin_marketplace::REGISTRY_ORIGIN
    ));
    interactive.selected = selected.min(interactive.items.len().saturating_sub(1));
    interactive.hovered = hovered;
    interactive
}

/// Build `/skills` picker from discovered skills.
pub fn build_skills_selector(skills: &[SkillListItem]) -> InteractiveState {
    let items = if skills.is_empty() {
        vec![
            InteractiveItem::new("__empty__", "No skills found")
                .with_description("Add SKILL.md under ~/.cortex/skills or .cortex/skills")
                .with_disabled(true),
        ]
    } else {
        skills
            .iter()
            .map(|skill| {
                InteractiveItem::new(&skill.name, format!("/{}", skill.name))
                    .with_description(skill.description.clone())
            })
            .collect()
    };
    InteractiveState::new(
        "Skills",
        items,
        InteractiveAction::Custom("skill-run".to_string()),
    )
    .with_search()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_selector_marks_current() {
        let state = build_mode_selector("Plan");
        assert_eq!(state.items.len(), 3);
        assert!(state.items[1].is_current);
        assert!(!state.items[0].is_current);
    }

    #[test]
    fn skills_selector_empty_is_disabled() {
        let state = build_skills_selector(&[]);
        assert!(state.items[0].disabled);
    }

    #[test]
    fn effort_alias_opens_model_radios_not_a_star_picker() {
        let state = build_effort_selector(Some("low"));
        assert_eq!(state.effort, Some(crate::interactive::EffortLevel::Low));
        let line = state.effort.expect("effort").radios_line();
        assert_eq!(line, "○ High Effort   ○ Medium Effort   ● Low Effort");
        assert!(!line.contains('★') && !line.contains("MAX"), "{line}");
    }
}
