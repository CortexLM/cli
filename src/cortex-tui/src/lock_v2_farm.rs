//! Farm batch 3 lock proofs: modes, MCP, hover, resume/clear, sandbox deny.
//!
//! Does not touch permission-prompt radios (COR-8).

#[cfg(test)]
mod tests {
    use crate::lock_v2::render_lock_v2_scene;
    use cortex_core::style::{ACCENT, BAR_HOVER};

    fn has_bg(
        frame: &crate::lock_proof::LockFrame,
        w: u16,
        h: u16,
        color: ratatui::style::Color,
    ) -> bool {
        for y in 0..h {
            for x in 0..w {
                if frame.buffer[(x, y)].bg == color {
                    return true;
                }
            }
        }
        false
    }

    #[test]
    fn modes_and_plan_handoff() {
        let plan = render_lock_v2_scene("mode-plan", 120, 40).expect("plan");
        assert!(
            plan.plain
                .contains("Plan — no files change until you approve"),
            "{}",
            plan.plain
        );
        assert!(plan.plain.contains("Plan · no edits"), "{}", plan.plain);
        assert!(
            plan.plain
                .contains("Describe what you want — Cortex drafts a plan first")
                || plan.plain.contains("drafts a plan first"),
            "{}",
            plan.plain
        );
        let ask = render_lock_v2_scene("mode-ask", 120, 40).expect("ask");
        assert!(ask.plain.contains("Ask · read-only"), "{}", ask.plain);
        assert!(
            ask.plain.contains("Ask about the codebase") || ask.plain.contains("read-only"),
            "{}",
            ask.plain
        );
        let agent = render_lock_v2_scene("mode-agent", 120, 40).expect("agent");
        assert!(
            agent.plain.contains("cargo test -p cortex-tui"),
            "{}",
            agent.plain
        );
        let confirm = render_lock_v2_scene("plan-confirm", 120, 40).expect("plan-confirm");
        assert!(
            confirm
                .plain
                .contains("Yes, switch to Agent mode and implement"),
            "{}",
            confirm.plain
        );
        assert!(confirm.plain.contains("keep planning"), "{}", confirm.plain);
        let cloud = render_lock_v2_scene("cloud-handoff", 120, 40).expect("cloud");
        assert!(
            cloud.plain.contains("Handed off to Cortex Cloud"),
            "{}",
            cloud.plain
        );
        let lower = format!("{}{}", plan.plain, ask.plain).to_ascii_lowercase();
        assert!(!lower.contains("claude"));
        assert!(!lower.contains("openai"));
    }

    #[test]
    fn mcp_list_add_toggle_and_drop() {
        let list = render_lock_v2_scene("mcp-servers", 120, 40).expect("mcp");
        assert!(
            list.plain.contains("MCP servers · 2 of 4 connected"),
            "{}",
            list.plain
        );
        assert!(list.plain.contains("github"), "{}", list.plain);
        assert!(list.plain.contains("filesystem"), "{}", list.plain);
        assert!(list.plain.contains("authenticating"), "{}", list.plain);
        assert!(list.plain.contains("token expired"), "{}", list.plain);
        assert!(list.plain.contains("/mcp"), "{}", list.plain);
        assert!(list.plain.contains("r:reconnect"), "{}", list.plain);
        assert!(list.plain.contains("a:add server"), "{}", list.plain);
        let narrow = render_lock_v2_scene("mcp-servers", 40, 12).expect("mcp-n");
        assert!(narrow.plain.contains("MCP servers"), "{}", narrow.plain);
        assert!(
            narrow.plain.contains("Esc:close") && !narrow.plain.contains("r:reconnect"),
            "{}",
            narrow.plain
        );
        let drop = render_lock_v2_scene("mcp-drop", 120, 40).expect("drop");
        assert!(drop.plain.contains("github dropped"), "{}", drop.plain);
        assert!(drop.plain.contains("Reconnecting"), "{}", drop.plain);
    }

    #[test]
    fn hover_uses_bar_hover_not_accent() {
        let footer = render_lock_v2_scene("footer-hover", 120, 40).expect("footer");
        assert!(
            has_bg(&footer, 120, 40, BAR_HOVER),
            "footer-hover needs #1A1A1A"
        );
        let idle = render_lock_v2_scene("footer-shortcuts", 120, 40).expect("idle-footer");
        assert_ne!(footer.ansi, idle.ansi);

        let slash = render_lock_v2_scene("slash-palette", 120, 40).expect("slash");
        assert!(has_bg(&slash, 120, 40, BAR_HOVER), "slash hover #1A1A1A");
        let models = render_lock_v2_scene("model-list-hover", 120, 40).expect("mlh");
        assert!(has_bg(&models, 120, 40, BAR_HOVER), "model-list-hover");
        let focused = render_lock_v2_scene("model-list", 120, 40).expect("ml");
        assert_ne!(models.ansi, focused.ansi);

        let settings = render_lock_v2_scene("settings-row-hover", 120, 40).expect("srh");
        assert!(has_bg(&settings, 120, 40, BAR_HOVER), "settings-row-hover");
        let perm = render_lock_v2_scene("permission-prompt-hover", 120, 40).expect("pph");
        assert!(has_bg(&perm, 120, 40, BAR_HOVER), "permission-prompt-hover");
        let prompt = render_lock_v2_scene("permission-prompt", 120, 40).expect("pp");
        assert!(prompt.plain.contains("Yes, run once"), "{}", prompt.plain);
        assert_ne!(perm.ansi, prompt.ansi);

        let mut hover_accent_on_unselected = false;
        for y in 0..40u16 {
            for x in 0..120u16 {
                let cell = &models.buffer[(x, y)];
                if cell.bg == BAR_HOVER && cell.fg == ACCENT {
                    hover_accent_on_unselected = true;
                }
            }
        }
        assert!(
            !hover_accent_on_unselected,
            "hover must not paint banner green"
        );
    }

    #[test]
    fn resume_clear_compact() {
        let resume = render_lock_v2_scene("resume-picker", 120, 40).expect("resume");
        assert!(
            resume.plain.contains("Type to search sessions"),
            "{}",
            resume.plain
        );
        assert!(
            resume.plain.contains("fix login redirect"),
            "{}",
            resume.plain
        );
        assert!(resume.plain.contains("/resume"), "{}", resume.plain);
        assert!(resume.plain.contains("f:favorite"), "{}", resume.plain);
        assert!(!resume.plain.contains("New Session"), "{}", resume.plain);
        let lower = resume.plain.to_ascii_lowercase();
        assert!(!lower.contains("anthropic"));
        assert!(!lower.contains("claude"));

        let clear = render_lock_v2_scene("clear-confirm", 120, 40).expect("clear");
        assert!(
            clear.plain.contains("Clear this conversation?"),
            "{}",
            clear.plain
        );
        assert!(
            clear
                .plain
                .contains("Git, files and config stay as they are"),
            "{}",
            clear.plain
        );
        assert!(clear.plain.contains("/clear"), "{}", clear.plain);
        assert!(clear.plain.contains("1 Yes, clear"), "{}", clear.plain);

        let compact = render_lock_v2_scene("compact-chat", 120, 40).expect("compact");
        assert!(
            compact.plain.contains("I'm Cortex") || compact.plain.contains("Cortex"),
            "{}",
            compact.plain
        );
    }

    #[test]
    fn sandbox_deny_matches_permission_radio_chrome() {
        let deny = render_lock_v2_scene("sandbox-deny", 120, 40).expect("deny");
        assert!(deny.plain.contains("Sandbox denied"), "{}", deny.plain);
        assert!(deny.plain.contains("1 Keep blocked"), "{}", deny.plain);
        assert!(deny.plain.contains("Allow once"), "{}", deny.plain);
        assert!(
            deny.plain.contains("Allow for this session"),
            "{}",
            deny.plain
        );
        assert!(
            deny.plain.contains("Choose an option above"),
            "{}",
            deny.plain
        );
        assert!(!deny.plain.contains("e:edit command"), "{}", deny.plain);

        let perms = render_lock_v2_scene("permissions-picker", 120, 40).expect("perms");
        assert!(
            perms
                .plain
                .contains("Permissions · how Cortex asks before acting"),
            "{}",
            perms.plain
        );
        assert!(perms.plain.contains("Smart"), "{}", perms.plain);
        assert!(perms.plain.contains("Enter:apply"), "{}", perms.plain);
        assert!(!perms.plain.contains("e:edit command"), "{}", perms.plain);
    }
}
