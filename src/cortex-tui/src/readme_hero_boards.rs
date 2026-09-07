//! README hero scene dispatch. Split out of [`crate::lock_boards`] so the
//! storyboard can grow without pushing that file past the source-policy
//! line-count gate.
//!
//! Painting stays on the signed lock chrome (banner-green focus). This
//! module does not restyle anything.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::lock_boards;

/// README hero beat: signed splash, typing, working, then local command boards.
#[derive(Debug, Clone, Copy)]
pub enum HeroScene<'a> {
    /// Dual-hairline splash with the idle placeholder.
    Splash,
    /// Splash chrome with `text` in the composer (partial or full prompt).
    Typing(&'a str),
    /// Submitted prompt plus the working indicator.
    Working,
    /// Slash palette (`/` typed) — local commands, not a cloud handoff.
    Palette,
    /// `/model` picker with effort radios.
    Model,
    /// Live Shell tool row (`✓` mint on passing tests).
    Shell,
    /// Back to the idle composer after the first user turn (no welcome card).
    Composer,
}

/// Paint one README-hero / lock frame from the signed chrome.
pub fn paint_hero(area: Rect, buf: &mut Buffer, scene: HeroScene<'_>) {
    match scene {
        HeroScene::Splash => lock_boards::render_lock_board("splash", area, buf),
        HeroScene::Typing(text) => lock_boards::paint_typed_splash(area, buf, text),
        HeroScene::Working => lock_boards::render_lock_board("working", area, buf),
        HeroScene::Palette => lock_boards::render_lock_board("palette", area, buf),
        HeroScene::Model => lock_boards::render_lock_board("model_full", area, buf),
        HeroScene::Shell => lock_boards::render_lock_board("shell", area, buf),
        HeroScene::Composer => lock_boards::paint_hero_composer(area, buf),
    }
}
