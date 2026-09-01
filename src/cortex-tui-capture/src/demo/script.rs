//! The storyboard for the README recording.
//!
//! The script is a plain list of [`Scene`] values plus how long each one is
//! held. Nothing here talks to the network: the recording is produced entirely
//! from mock state, which is what lets it run in CI where the TUI cannot log in.

use super::scene::{Scene, Status, TimelineBlock, ToolRow};

/// A scene together with the number of frames it stays on screen.
#[derive(Debug, Clone)]
pub struct Beat {
    /// Short identifier, used for frame labels and tests.
    pub label: String,
    /// The state to render.
    pub scene: Scene,
    /// How many output frames this scene occupies.
    pub hold: u32,
}

impl Beat {
    fn new(label: &str, scene: Scene, hold: u32) -> Self {
        Self {
            label: label.to_string(),
            scene,
            hold,
        }
    }
}

/// The prompt the demo user types.
pub const DEMO_PROMPT: &str = "add a /healthz endpoint and cover it with a test";

/// Build the full storyboard.
///
/// The result is deterministic: the same input always produces the same frames,
/// so the recording can be regenerated and diffed.
pub fn storyboard() -> Vec<Beat> {
    let mut beats = Vec::new();
    let mut scene = Scene::welcome();

    // 1. Idle welcome card, with the cursor blinking in an empty composer.
    for i in 0..6 {
        scene.cursor_on = i % 2 == 0;
        beats.push(Beat::new("welcome", scene.clone(), 2));
    }

    // 2. The user types the prompt. Several characters land per frame so the
    //    typing reads as brisk rather than laboured.
    let chars: Vec<char> = DEMO_PROMPT.chars().collect();
    let mut typed = String::new();
    for chunk in chars.chunks(2) {
        typed.extend(chunk);
        scene.composer = typed.clone();
        scene.cursor_on = true;
        beats.push(Beat::new("typing", scene.clone(), 1));
    }

    // 3. A short pause on the completed prompt before it is submitted.
    beats.push(Beat::new("prompt-ready", scene.clone(), 6));

    // 4. Enter: the prompt moves into the timeline and the turn starts.
    scene.composer.clear();
    scene.cursor_on = false;
    scene.push(TimelineBlock::User(DEMO_PROMPT.to_string()));
    for i in 0..4u32 {
        scene.status = Some(Status::new("Thinking", i / 2, i as usize));
        beats.push(Beat::new("thinking", scene.clone(), 2));
    }

    // 5. Tool calls, each shown running and then completed.
    let mut spinner = 4usize;
    let mut elapsed = 2u32;
    let tools = [
        ("Grep", "\"healthz\" in src/", "no matches in src/routes"),
        ("Read", "src/routes/mod.rs", "128 lines"),
        ("Write", "src/routes/health.rs", "+18 −0"),
        ("Edit", "src/routes/mod.rs", "+2 −0"),
        ("Write", "tests/health.rs", "+21 −0"),
        (
            "Shell",
            "cargo test --test health",
            "test result: ok. 14 passed; 0 failed",
        ),
    ];

    for (name, summary, result) in tools {
        scene.push(TimelineBlock::Tool(ToolRow::running(name, summary)));
        for _ in 0..2 {
            spinner += 1;
            elapsed += 1;
            scene.status = Some(Status::new(&format!("Executing {name}"), elapsed, spinner));
            beats.push(Beat::new("tool-running", scene.clone(), 2));
        }
        scene.complete_last_tool(result);
        spinner += 1;
        scene.status = Some(Status::new("Thinking", elapsed, spinner));
        beats.push(Beat::new("tool-done", scene.clone(), 2));
    }

    // 6. The agent's answer streams in, one line at a time.
    let reply = [
        "Added a `/healthz` route that returns 200 with a JSON body, registered it on",
        "the router, and covered it with an integration test.",
        "",
        "  src/routes/health.rs   +18 −0",
        "  src/routes/mod.rs       +2 −0",
        "  tests/health.rs        +21 −0",
        "",
        "`cargo test --test health` passes: 14 tests, 0 failures.",
    ];

    let mut streamed: Vec<String> = Vec::new();
    scene.push(TimelineBlock::Agent(Vec::new()));
    for line in reply {
        streamed.push(line.to_string());
        if let Some(TimelineBlock::Agent(body)) = scene.blocks.last_mut() {
            body.clone_from(&streamed);
        }
        spinner += 1;
        scene.status = Some(Status::new("Streaming..", elapsed, spinner));
        beats.push(Beat::new("streaming", scene.clone(), 2));
    }

    // 7. Turn complete: the composer takes focus again and the loop rests here.
    scene.status = None;
    for i in 0..7 {
        scene.cursor_on = i % 2 == 0;
        beats.push(Beat::new("idle", scene.clone(), 3));
    }

    beats
}

/// Total number of output frames the storyboard produces.
pub fn total_frames(beats: &[Beat]) -> u32 {
    beats.iter().map(|beat| beat.hold).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storyboard_runs_between_eight_and_fifteen_seconds_at_twelve_fps() {
        let beats = storyboard();
        let seconds = f64::from(total_frames(&beats)) / 12.0;
        assert!(
            (8.0..=15.0).contains(&seconds),
            "demo loop is {seconds:.1}s, expected 8-15s"
        );
    }

    #[test]
    fn storyboard_covers_the_required_beats() {
        let beats = storyboard();
        for expected in [
            "welcome",
            "typing",
            "thinking",
            "tool-running",
            "tool-done",
            "streaming",
            "idle",
        ] {
            assert!(
                beats.iter().any(|beat| beat.label == expected),
                "storyboard is missing the '{expected}' beat"
            );
        }
    }

    #[test]
    fn storyboard_ends_on_an_idle_composer() {
        let beats = storyboard();
        let last = beats.last().expect("storyboard is not empty");
        assert_eq!(last.label, "idle");
        assert!(last.scene.status.is_none());
        assert!(last.scene.composer.is_empty());
    }

    #[test]
    fn typing_ends_with_the_full_prompt() {
        let beats = storyboard();
        let last_typed = beats
            .iter()
            .rev()
            .find(|beat| beat.label == "prompt-ready")
            .expect("prompt-ready beat");
        assert_eq!(last_typed.scene.composer, DEMO_PROMPT);
    }
}
