//! Scene model for the recorded demo session.
//!
//! A [`Scene`] is one fully-described state of the Cortex Code session view.
//! The demo script produces an ordered list of scenes; the renderer turns each
//! one into a frame. Keeping the state declarative means the storyboard can be
//! asserted in tests without going near a real terminal or the live API.

/// Lifecycle of a tool row in the timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolState {
    /// The tool is still executing.
    Running,
    /// The tool finished successfully.
    Done,
}

/// A single tool invocation as it appears in the timeline.
#[derive(Debug, Clone)]
pub struct ToolRow {
    /// Tool name as the harness reports it (`Grep`, `Read`, `Edit`, `Execute`).
    pub name: String,
    /// Short argument summary shown next to the tool name.
    pub summary: String,
    /// Result line rendered under the tool row once it completes.
    pub result: Option<String>,
    /// Current lifecycle state.
    pub state: ToolState,
}

impl ToolRow {
    /// A tool row that is still running.
    pub fn running(name: &str, summary: &str) -> Self {
        Self {
            name: name.to_string(),
            summary: summary.to_string(),
            result: None,
            state: ToolState::Running,
        }
    }

    /// Mark this row complete with a result summary.
    pub fn completed(mut self, result: &str) -> Self {
        self.result = Some(result.to_string());
        self.state = ToolState::Done;
        self
    }
}

/// One entry in the session timeline.
#[derive(Debug, Clone)]
pub enum TimelineBlock {
    /// A prompt submitted by the user.
    User(String),
    /// Agent prose, already wrapped to the target width.
    Agent(Vec<String>),
    /// A tool call row.
    Tool(ToolRow),
}

/// The working indicator shown between the timeline and the composer.
#[derive(Debug, Clone)]
pub struct Status {
    /// Header text, matching the session view's status headers.
    pub header: String,
    /// Seconds elapsed for the current turn.
    pub elapsed_secs: u32,
    /// Index into the spinner frame table.
    pub spinner: usize,
}

/// Spinner frames used by the working indicator.
pub const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

impl Status {
    /// Create a status row.
    pub fn new(header: &str, elapsed_secs: u32, spinner: usize) -> Self {
        Self {
            header: header.to_string(),
            elapsed_secs,
            spinner,
        }
    }

    /// The spinner glyph for this status.
    pub fn spinner_glyph(&self) -> &'static str {
        SPINNER_FRAMES[self.spinner % SPINNER_FRAMES.len()]
    }
}

/// A complete, renderable state of the session view.
#[derive(Debug, Clone)]
pub struct Scene {
    /// Workspace path shown on the welcome card.
    pub workspace: String,
    /// API host the session is bound to.
    pub endpoint: String,
    /// Operation mode indicator (`BUILD`, `PLAN`, `SPEC`).
    pub mode: String,
    /// Permission mode label (`yolo`, `low`, `medium`, `high`).
    pub autonomy: String,
    /// Timeline contents, oldest first.
    pub blocks: Vec<TimelineBlock>,
    /// Current composer text.
    pub composer: String,
    /// Whether the composer cursor is drawn on this frame.
    pub cursor_on: bool,
    /// Working indicator, when a turn is in flight.
    pub status: Option<Status>,
}

impl Scene {
    /// An empty session, as it looks the moment the TUI opens.
    pub fn welcome() -> Self {
        Self {
            workspace: "~/code/acme-api".to_string(),
            endpoint: "api.cortex.foundation".to_string(),
            mode: "BUILD".to_string(),
            autonomy: "medium".to_string(),
            blocks: Vec::new(),
            composer: String::new(),
            cursor_on: true,
            status: None,
        }
    }

    /// Whether the welcome card should be drawn instead of a timeline.
    pub fn is_empty_session(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Append a timeline block.
    pub fn push(&mut self, block: TimelineBlock) {
        self.blocks.push(block);
    }

    /// Replace the most recent tool row with a completed version.
    ///
    /// Panics if the last block is not a tool row; the storyboard is static, so
    /// a mismatch is a programming error rather than a runtime condition.
    pub fn complete_last_tool(&mut self, result: &str) {
        match self.blocks.last_mut() {
            Some(TimelineBlock::Tool(row)) => {
                row.result = Some(result.to_string());
                row.state = ToolState::Done;
            }
            _ => panic!("complete_last_tool called when the last block is not a tool row"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_scene_has_no_timeline() {
        let scene = Scene::welcome();
        assert!(scene.is_empty_session());
        assert!(scene.composer.is_empty());
    }

    #[test]
    fn completing_a_tool_row_records_the_result() {
        let mut scene = Scene::welcome();
        scene.push(TimelineBlock::Tool(ToolRow::running("Grep", "\"healthz\"")));
        scene.complete_last_tool("3 matches");

        match scene.blocks.last() {
            Some(TimelineBlock::Tool(row)) => {
                assert_eq!(row.state, ToolState::Done);
                assert_eq!(row.result.as_deref(), Some("3 matches"));
            }
            other => panic!("expected a tool row, got {other:?}"),
        }
    }

    #[test]
    fn spinner_glyph_wraps_around() {
        let status = Status::new("Thinking", 2, SPINNER_FRAMES.len() + 1);
        assert_eq!(status.spinner_glyph(), SPINNER_FRAMES[1]);
    }
}
