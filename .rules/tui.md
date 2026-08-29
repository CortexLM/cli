# TUI and responsive layout

- The TUI must actually run. `cargo build -p cortex-cli` produces `Cortex`; `Cortex` with a TTY starts the interactive UI.
- Use the existing `cortex-tui-framework` crates (buffer, layout, widgets, input, text, terminal). Do not add a second widget toolkit.
- Layouts must reflow. Prefer flex/constraint layout over hardcoded 80×24 coordinates. Test a narrow (40×12) and a wide (120×40) viewport when you change a surface.
- Every surface you change needs:
  1. a unit test for the logic, and
  2. a headless snapshot or ratatui-style test (`cortex-tui-capture` `MockTerminal` / `FrameCapture`, or a `cortex-tui-buffer` render + assert).
- No mocks that report success. A snapshot of an error state must contain the product-facing error string.
- Do not block the render thread on network I/O. Agent turns, login, and tool exec stay on the engine / tokio tasks.
- Color and style go through `cortex-tui-core`. Do not hardcode ANSI in widgets except in capture tests that assert the buffer.
