//! Splash legend + version-width helpers.
//!
//! Split out of [`crate::lock_boards`] so a longer package version can
//! ellipsis without growing that file past the source-policy line-count
//! gate. At 40 columns the mid keystroke legend is kept; `& cloud` is
//! added when it fits. The version shortens — the legend does not.

/// Full empty-session hints, including the cloud slot when width allows.
const LAUNCH_HINTS: &str = crate::views::minimal_session::EMPTY_SESSION_HINTS;
/// Form that fits a 40-column lock (`& cloud` dropped).
const LAUNCH_HINTS_NARROW: &str = "/ commands · @ files · ! shell";

/// Join `v{version}` to a hint legend, ellipsizing the version — never the
/// legend — when the pair is wider than `width`.
pub(crate) fn splash_legend(version: &str, width: usize) -> String {
    let ver = format!("v{version}");
    for legend in [LAUNCH_HINTS, LAUNCH_HINTS_NARROW] {
        let line = format!("{ver} · {legend}");
        if line.chars().count() <= width {
            return line;
        }
    }
    // Keep `/ commands · @ files · ! shell` (and `& cloud` when it fits).
    // A longer package version is shortened; the keystroke legend is not.
    for legend in [LAUNCH_HINTS, LAUNCH_HINTS_NARROW] {
        let suffix = format!(" · {legend}");
        let suffix_len = suffix.chars().count();
        if suffix_len >= width {
            continue;
        }
        let shown = ellipsis_prefix(&ver, width - suffix_len);
        if shown.is_empty() {
            continue;
        }
        return format!("{shown}{suffix}");
    }
    LAUNCH_HINTS_NARROW.to_string()
}

fn ellipsis_prefix(text: &str, budget: usize) -> String {
    if text.chars().count() <= budget {
        return text.to_string();
    }
    if budget == 0 {
        return String::new();
    }
    if budget == 1 {
        return "…".to_string();
    }
    format!("{}…", text.chars().take(budget - 1).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splash_legend_keeps_mid_hints_when_version_grows() {
        for version in [env!("CARGO_PKG_VERSION"), "0.1.10", "10.20.30-rc.1"] {
            let legend = splash_legend(version, 40);
            assert!(
                legend.contains("/ commands · @ files · ! shell"),
                "40-col splash must keep mid hints for v{version}: {legend}"
            );
            assert!(
                !legend.contains("& cloud"),
                "40-col splash shortens before & cloud: {legend}"
            );
        }
    }

    #[test]
    fn splash_legend_adds_cloud_when_width_allows() {
        let legend = splash_legend("0.1.10", 120);
        assert!(
            legend.contains("/ commands · @ files · ! shell · & cloud"),
            "wide splash keeps the cloud slot: {legend}"
        );
        assert!(
            legend.starts_with("v0.1.10 · "),
            "wide splash keeps the full version: {legend}"
        );
    }

    #[test]
    fn splash_legend_ellipsizes_version_not_legend() {
        let exact = splash_legend("0.1.10", 40);
        assert_eq!(exact, "v0.1.10 · / commands · @ files · ! shell");
        assert_eq!(exact.chars().count(), 40);

        let overflow = splash_legend("10.20.30-rc.1", 40);
        assert!(
            overflow.contains("/ commands · @ files · ! shell"),
            "overflow must keep the mid legend: {overflow}"
        );
        assert!(
            overflow.contains('…'),
            "a longer version ellipsizes: {overflow}"
        );
        assert!(overflow.chars().count() <= 40, "{overflow}");
    }
}
