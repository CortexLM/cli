# Structure

- This is one Cargo workspace. Shared versions, edition, rust-version, and license live in `[workspace.package]`.
- New crates go under `src/<name>` (or `cortex-tui-framework/crates/` for TUI primitives) and must be listed in the root `Cargo.toml`.
- Do not add a parallel package manager, a second binary name, or a second version file. Version sources are `VERSION_CLI`, `[workspace.package].version`, and `src/cortex-cli/VERSION`.
- Keep the agent loop in `cortex-engine`, protocol types in `cortex-protocol`, CLI parsing in `cortex-cli`, and rendering in `cortex-tui`. Do not grow a god crate.
- Delete dead stubs. Do not leave `todo!()` public APIs or crates that only re-export empty modules.
- Edition is `2024`. rust-version tracks current stable (see `rust-toolchain.toml`).
- `unsafe_code` is allowed only where sandboxing requires it. New crates default to safe Rust.
