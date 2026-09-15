//! Settings-modal lock v2 scenes.
//!
//! Split out of [`crate::lock_v2_boards`] so that file stays under the
//! source-policy line-count target. Each scene opens the real settings modal
//! and tunes the row the board documents.

use crate::app::AppState;
use crate::lock_v2_scenes::open_settings;
use crate::widgets::settings_modal::SettingsRowKind;

/// Apply a settings-modal lock scene. Returns `false` when `id` is not one.
pub(crate) fn apply_settings_scene(id: &str, state: &mut AppState) -> bool {
    match id {
        "settings-appearance" => open_settings(state, |_| {}),
        "settings-mouse" => open_settings(state, |modal| {
            if let Some(i) = modal
                .visible_rows()
                .iter()
                .position(|r| r.id == "mouse_capture")
            {
                modal.selected = i;
                modal.scroll = modal
                    .visible_rows()
                    .iter()
                    .position(|r| r.id == "mouse" || r.label == "Mouse")
                    .unwrap_or(i.saturating_sub(1));
            }
        }),
        "settings-row-hover" => open_settings(state, |modal| {
            modal.selected = 1; // Compact mode
            if let Some(i) = modal
                .visible_rows()
                .iter()
                .position(|r| r.id == "timestamps")
            {
                modal.hovered = Some(i);
            }
        }),
        "settings-search" => open_settings(state, |modal| {
            modal.search = "scro".into();
            modal.search_focused = true;
            modal.selected = 0;
            if let Some(i) = modal
                .visible_rows()
                .iter()
                .position(|r| r.kind != SettingsRowKind::Category)
            {
                modal.selected = i;
            }
        }),
        "settings-theme-submenu" => open_settings(state, |modal| {
            modal.theme_open = true;
            modal.theme_selected = 0;
        }),
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_settings_ids_are_claimed() {
        for id in [
            "settings-appearance",
            "settings-mouse",
            "settings-row-hover",
            "settings-search",
            "settings-theme-submenu",
        ] {
            let mut state = crate::lock_v2_scenes::lock_app();
            assert!(apply_settings_scene(id, &mut state), "{id}");
        }
        let mut state = crate::lock_v2_scenes::lock_app();
        assert!(!apply_settings_scene("welcome-cortex", &mut state));
    }

    #[test]
    fn the_search_scene_focuses_the_search_field() {
        let mut state = crate::lock_v2_scenes::lock_app();
        assert!(apply_settings_scene("settings-search", &mut state));
        let modal = state.settings_modal.as_ref().expect("settings modal");
        assert_eq!(modal.search, "scro");
        assert!(modal.search_focused);
    }
}
