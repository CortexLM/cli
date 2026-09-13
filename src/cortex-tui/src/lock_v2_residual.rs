//! Residual lock v2 proofs for `/shortcuts`, composer, density, and live states.
//!
//! Permission-prompt radios are untouched.

#[cfg(test)]
mod tests {
    use crate::lock_v2::render_lock_v2_scene;
    use cortex_core::style::{ACCENT, BORDER_FOCUS, HAIRLINE};

    #[test]
    fn shortcuts_sheet_opens_at_both_sizes() {
        for (w, h) in [(120u16, 40u16), (40u16, 12u16)] {
            let frame = render_lock_v2_scene("shortcuts-overlay", w, h).expect("sheet");
            assert!(
                frame.plain.contains("Shortcuts"),
                "{w}x{h}:\n{}",
                frame.plain
            );
            assert!(
                frame.plain.contains("Ctrl+x") || frame.plain.contains("Esc"),
                "{w}x{h}:\n{}",
                frame.plain
            );
            let mut accent = false;
            for y in 0..h {
                for x in 0..w {
                    if frame.buffer[(x, y)].fg == ACCENT {
                        accent = true;
                    }
                }
            }
            assert!(accent, "selected binding uses #1F4945 at {w}x{h}");
        }
    }

    #[test]
    fn composer_hover_lifts_hairline() {
        let idle = render_lock_v2_scene("composer-empty", 120, 40).expect("empty");
        let hover = render_lock_v2_scene("composer-hover", 120, 40).expect("hover");
        let mut hover_focus = false;
        let mut idle_hair = false;
        for y in 0..40u16 {
            for x in 0..120u16 {
                if hover.buffer[(x, y)].fg == BORDER_FOCUS
                    && hover.buffer[(x, y)].symbol().contains('─')
                {
                    hover_focus = true;
                }
                if idle.buffer[(x, y)].fg == HAIRLINE && idle.buffer[(x, y)].symbol().contains('─')
                {
                    idle_hair = true;
                }
            }
        }
        assert!(idle_hair, "idle composer keeps hairline gray");
        assert!(hover_focus, "hover lifts hairline to #525252");
        assert_ne!(idle.ansi, hover.ansi);
    }

    #[test]
    fn composer_placeholder_typing_multiline() {
        let empty = render_lock_v2_scene("composer-empty", 120, 40).expect("empty");
        assert!(
            empty.plain.contains("Plan, search, build anything"),
            "{}",
            empty.plain
        );
        let typing = render_lock_v2_scene("composer-typing", 120, 40).expect("typing");
        assert!(
            typing
                .plain
                .contains("add retry with backoff to the api client"),
            "{}",
            typing.plain
        );
        assert!(typing.plain.contains("Alt+Enter"), "{}", typing.plain);
        let multi = render_lock_v2_scene("composer-multiline", 120, 40).expect("multi");
        assert!(multi.plain.contains("max 3 attempts"), "{}", multi.plain);
        assert!(multi.plain.contains("Alt+Enter"), "{}", multi.plain);
        let blink = render_lock_v2_scene("composer-typing-blink", 120, 40).expect("blink");
        assert_ne!(typing.ansi, blink.ansi);
    }

    #[test]
    fn code_diff_markdown_density() {
        let fence = render_lock_v2_scene("code-fence", 120, 40).expect("fence");
        assert!(fence.plain.contains("rust"), "{}", fence.plain);
        assert!(fence.plain.contains("with_retry"), "{}", fence.plain);
        assert!(
            fence.plain.contains('─'),
            "language tag hairline:\n{}",
            fence.plain
        );
        let table = render_lock_v2_scene("md-table", 120, 40).expect("table");
        assert!(table.plain.contains("Cortex Mini 1"), "{}", table.plain);
        assert!(table.plain.contains("Cortex Max 1"), "{}", table.plain);
        let diff = render_lock_v2_scene("diff-hunk", 120, 40).expect("diff");
        assert!(diff.plain.contains("Edit"), "{}", diff.plain);
        assert!(
            diff.plain.contains('+') && diff.plain.contains('-'),
            "{}",
            diff.plain
        );
        let narrow = render_lock_v2_scene("diff-hunk", 40, 12).expect("diff-narrow");
        assert!(
            narrow.plain.contains("Edit") || narrow.plain.contains('+'),
            "{}",
            narrow.plain
        );
    }

    #[test]
    fn streaming_states_are_distinct() {
        let thinking = render_lock_v2_scene("session-thinking-live", 120, 40).expect("think");
        let interrupt = render_lock_v2_scene("interrupt-stopped", 120, 40).expect("stop");
        let todos = render_lock_v2_scene("todos", 120, 40).expect("todos");
        let question = render_lock_v2_scene("question", 120, 40).expect("q");
        assert!(thinking.plain.contains("Thinking"), "{}", thinking.plain);
        assert!(interrupt.plain.contains("Stopped"), "{}", interrupt.plain);
        assert!(
            interrupt.plain.contains("Reply, or") || interrupt.plain.contains("Stopped"),
            "{}",
            interrupt.plain
        );
        assert!(todos.plain.contains("Working 2/5"), "{}", todos.plain);
        assert!(todos.plain.contains('✓'), "{}", todos.plain);
        assert!(todos.plain.contains('›'), "{}", todos.plain);
        assert!(todos.plain.contains('○'), "{}", todos.plain);
        assert!(
            question.plain.contains("Which failures should be retried?"),
            "{}",
            question.plain
        );
        assert!(
            question.plain.contains("Timeouts and 5xx"),
            "{}",
            question.plain
        );
        let frames = [
            thinking.ansi.as_str(),
            interrupt.ansi.as_str(),
            todos.ansi.as_str(),
            question.ansi.as_str(),
        ];
        for i in 0..frames.len() {
            for j in (i + 1)..frames.len() {
                assert_ne!(
                    frames[i], frames[j],
                    "states {i} and {j} must stay distinct"
                );
            }
        }
        let think_n = render_lock_v2_scene("session-thinking-live", 40, 12).expect("think-n");
        let stop_n = render_lock_v2_scene("interrupt-stopped", 40, 12).expect("stop-n");
        assert_ne!(think_n.ansi, stop_n.ansi);
        assert!(think_n.plain.contains("Thinking"), "{}", think_n.plain);
        assert!(stop_n.plain.contains("Stopped"), "{}", stop_n.plain);
    }

    #[test]
    fn model_picker_matches_lock_copy() {
        let list = render_lock_v2_scene("model-list", 120, 40).expect("list");
        assert!(
            list.plain.contains("Fast default for everyday coding"),
            "{}",
            list.plain
        );
        assert!(
            list.plain.contains("Deeper reasoning for hard changes"),
            "{}",
            list.plain
        );
        assert!(
            list.plain.contains("Longest context") && list.plain.contains("MAX"),
            "{}",
            list.plain
        );
        assert!(list.plain.contains("current"), "{}", list.plain);
        assert!(!list.plain.contains("200K ctx"), "{}", list.plain);
        let lower = list.plain.to_ascii_lowercase();
        assert!(!lower.contains("claude"), "{}", list.plain);
        assert!(!lower.contains("anthropic"), "{}", list.plain);
        let effort = render_lock_v2_scene("model-effort-high", 120, 40).expect("effort");
        assert!(
            effort
                .plain
                .contains("Deepest reasoning — best for hard, multi-file changes"),
            "{}",
            effort.plain
        );
        assert!(
            effort.plain.contains("Cortex Mini 1 (medium)"),
            "{}",
            effort.plain
        );
        assert!(effort.plain.contains("Tab"), "{}", effort.plain);
        let narrow = render_lock_v2_scene("model-effort-high", 40, 12).expect("narrow");
        assert!(narrow.plain.contains("High Effort"), "{}", narrow.plain);
        assert!(
            narrow.plain.contains("Esc") && !narrow.plain.contains("Tab:back"),
            "{}",
            narrow.plain
        );
        let hover = render_lock_v2_scene("model-effort-hover", 120, 40).expect("hover");
        assert!(hover.plain.contains("Medium Effort"), "{}", hover.plain);
        assert_ne!(effort.ansi, hover.ansi);
    }
}
