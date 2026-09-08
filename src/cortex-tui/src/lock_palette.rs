//! Palette audit helpers for the verification MCP and green-lock gates.
//!
//! Split out of [`crate::lock_proof`] and [`crate::lock_v2`] the same way
//! [`crate::splash_chrome`] was extracted from lock boards: the signed accent
//! is banner green `#1F4945`, and those files must stay at their source-policy
//! line-count baseline. Historical violet `#A78BFA`, wash `#221A38`, and gold
//! `#C9A95C` are never painted.

use ratatui::buffer::Buffer;
use ratatui::style::Color;
use serde::Serialize;

/// Banned / reserved chrome pixels counted for `lock.palette_audit`.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct PaletteCounts {
    pub accent_px: u32,
    pub violet_px: u32,
    pub wash_px: u32,
    pub gold_px: u32,
    pub mint_px: u32,
    pub cyan_px: u32,
}

const VIOLET: (u8, u8, u8) = (0xA7, 0x8B, 0xFA);
const WASH: (u8, u8, u8) = (0x22, 0x1A, 0x38);
const GOLD: (u8, u8, u8) = (0xC9, 0xA9, 0x5C);
const MINT: (u8, u8, u8) = (0x00, 0xF5, 0xD4);
const CYAN: (u8, u8, u8) = (0x00, 0xFF, 0xFF);
const ACCENT_RGB: (u8, u8, u8) = (0x1F, 0x49, 0x45);

fn rgb_of(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Rgb(r, g, b) => Some((r, g, b)),
        Color::Cyan => Some(CYAN),
        _ => None,
    }
}

/// Count reserved / banned chrome pixels in a rendered buffer.
pub fn count_palette(buffer: &Buffer) -> PaletteCounts {
    let mut counts = PaletteCounts::default();
    let area = buffer.area;
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            let cell = &buffer[(x, y)];
            for color in [cell.fg, cell.bg] {
                let Some(rgb) = rgb_of(color) else {
                    continue;
                };
                if rgb == ACCENT_RGB {
                    counts.accent_px += 1;
                } else if rgb == VIOLET {
                    counts.violet_px += 1;
                } else if rgb == WASH {
                    counts.wash_px += 1;
                } else if rgb == GOLD {
                    counts.gold_px += 1;
                } else if rgb == MINT {
                    counts.mint_px += 1;
                } else if rgb == CYAN {
                    counts.cyan_px += 1;
                }
            }
        }
    }
    counts
}

/// Paint one retired-violet cell. Used by the verify MCP violet fixture.
pub fn inject_violet_cell(buffer: &mut Buffer, x: u16, y: u16) {
    buffer[(x, y)].set_fg(Color::Rgb(VIOLET.0, VIOLET.1, VIOLET.2));
}

impl PaletteCounts {
    /// True when any retired chrome colour is present.
    pub fn has_banned(&self) -> bool {
        self.violet_px + self.wash_px + self.gold_px + self.mint_px + self.cyan_px > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock_proof::{LockFrame, lock_scene_ids, render_lock_scene};
    use crate::lock_v2::{LOCK_V2_NARROW_IDS, LOCK_V2_WIDE_IDS, render_lock_v2_scene};
    use ratatui::layout::Rect;

    const SIZES: [(u16, u16); 2] = [(40, 12), (120, 40)];

    /// Mint, navy, historical lock violet, retired wash, thinking gold — SGR tuples.
    const BANNED_ANSI: [&str; 7] = [
        "0;245;212",
        "26;51;48",
        "0;255;163",
        "10;22;40",
        "167;139;250",
        "34;26;56",
        "201;169;92",
    ];

    /// `#A78BFA` violet, `#221A38` wash, `#C9A95C` gold.
    const BANNED_RGB: [(u8, u8, u8); 3] = [(167, 139, 250), (34, 26, 56), (201, 169, 92)];

    const RETIRED: [Color; 3] = [
        Color::Rgb(167, 139, 250),
        Color::Rgb(34, 26, 56),
        Color::Rgb(201, 169, 92),
    ];

    const ROUNDED: &[char] = &['╭', '╮', '╰', '╯'];

    fn is_retired_rgb(r: u8, g: u8, b: u8) -> bool {
        BANNED_RGB.contains(&(r, g, b))
    }

    fn is_hairline_box_edge(row: &str) -> bool {
        row.contains('─') && (row.contains('╭') || row.contains('╰'))
    }

    fn row_text(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect()
    }

    fn count_retired_style(frame: &LockFrame) -> u32 {
        let mut retired = 0u32;
        for y in 0..frame.buffer.area.height {
            for x in 0..frame.buffer.area.width {
                let cell = &frame.buffer[(x, y)];
                if let Some(Color::Rgb(r, g, b)) = cell.style().fg {
                    if is_retired_rgb(r, g, b) {
                        retired += 1;
                    }
                }
                if let Some(Color::Rgb(r, g, b)) = cell.style().bg {
                    if is_retired_rgb(r, g, b) {
                        retired += 1;
                    }
                }
            }
        }
        retired
    }

    fn count_retired_cells(frame: &LockFrame) -> u32 {
        let mut retired = 0u32;
        for y in 0..frame.buffer.area.height {
            for x in 0..frame.buffer.area.width {
                let cell = &frame.buffer[(x, y)];
                if RETIRED.contains(&cell.fg) || RETIRED.contains(&cell.bg) {
                    retired += 1;
                }
            }
        }
        retired
    }

    #[test]
    fn violet_cell_fixture_fails_palette_audit() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 4, 2));
        assert!(!count_palette(&buffer).has_banned());
        inject_violet_cell(&mut buffer, 1, 0);
        let counts = count_palette(&buffer);
        assert!(counts.has_banned());
        assert!(counts.violet_px >= 1);
    }

    #[test]
    fn v1_retired_palette_is_absent() {
        let mut retired = 0u32;
        for id in lock_scene_ids() {
            for size in SIZES {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                for banned in BANNED_ANSI {
                    assert!(
                        !frame.ansi.contains(banned),
                        "{id} paints banned color {banned} at {size:?}"
                    );
                }
                retired += count_retired_style(&frame);
                for y in 0..frame.buffer.area.height {
                    for x in 0..frame.buffer.area.width {
                        if let Some(Color::Rgb(r, g, b)) = frame.buffer[(x, y)].style().bg {
                            assert!(
                                r == g && g == b,
                                "{id} paints a tinted background {r},{g},{b} at {size:?} ({x},{y})"
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(
            retired, 0,
            "retired violet/wash/gold cells in lock v1 frames"
        );
    }

    #[test]
    fn v1_rounded_glyphs_stay_on_hairline_boxes() {
        for id in lock_scene_ids() {
            for size in SIZES {
                let frame = render_lock_scene(id, size.0, size.1).expect(id);
                let buf = &frame.buffer;
                for y in 0..buf.area.height {
                    let row = row_text(buf, y);
                    for x in 0..buf.area.width {
                        let Some(ch) = buf[(x, y)].symbol().chars().next() else {
                            continue;
                        };
                        if !ROUNDED.contains(&ch) {
                            continue;
                        }
                        assert!(
                            is_hairline_box_edge(&row),
                            "{id} paints rounded `{ch}` off a hairline box at {size:?} ({x},{y}):\n{row}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn v2_retired_palette_is_absent() {
        let mut retired = 0u32;
        for (width, height, ids) in [
            (120u16, 40u16, LOCK_V2_WIDE_IDS),
            (40u16, 12u16, LOCK_V2_NARROW_IDS),
        ] {
            for id in ids {
                let frame =
                    render_lock_v2_scene(id, width, height).unwrap_or_else(|e| panic!("{id}: {e}"));
                retired += count_retired_cells(&frame);
            }
        }
        assert_eq!(
            retired, 0,
            "retired violet/wash/gold cells in lock v2 frames"
        );
    }
}
