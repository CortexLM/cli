# Testing

- `cargo test --workspace` must pass before merge.
- No mocks that report success. If a test cannot exercise a backend, it asserts the product-facing failure, or it is an offline unit test of pure logic.
- TUI: for every surface you change, add (1) a unit test and (2) a headless snapshot or ratatui-style render test.
- Prefer `cortex-tui-capture::{MockTerminal, FrameCapture}` and `cortex-tui-buffer` assertions over spinning a real PTY unless you are testing the terminal backend.
- Network tests that hit `api.cortex.foundation` must be ignored by default or gated on `CORTEX_LIVE_API=1`. CI stays deterministic.
- `cargo audit` is required. Ignore an advisory only in `.cargo/audit.toml` with a comment that names the crate and the reason.
- Do not `#[ignore]` a failing test to go green. Fix it or delete the dead test.
