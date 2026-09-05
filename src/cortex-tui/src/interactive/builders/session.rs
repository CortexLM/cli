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

/// One skill row for `/skills`.
pub struct SkillListItem {
    pub name: String,
    pub description: String,
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
