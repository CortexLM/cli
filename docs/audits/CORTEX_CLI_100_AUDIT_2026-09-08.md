# Cortex CLI — 100 % TUI + MCP audit (2026-09-08)

Docs-only audit of Cortex CLI / Cortex Code (`CortexLM/cli`) at the tip of
`main` (`3035361`, tag `v0.1.10`, `VERSION_CLI` = `0.1.10`). It answers one
question for Mathis — *does the CLI work at 100 % against the product and
Designer cli locks?* — and gives the fix agents an ordered, file-cited plan so
nothing has to be guessed.

Author: echobt. Status marker at the end of this file: `CLI_100_AUDIT_READY`.

Short answer: **no, not yet.** The chrome is green-locked in code and the
headless lock suite is green (1 062 / 1 062 `cortex-tui` tests), but (1) a
fresh install cannot complete its first turn without an undocumented env var,
(2) every committed lock PNG pack is still the historical violet chrome and the
v1 render script *asserts* violet, (3) one startup request sends the bearer to
production regardless of `CORTEX_API_URL`, and (4) there is no verification
MCP — the `cortex mcp-server` subcommand is a stub. Details, evidence and the
ordered fix plan follow.

---

## 0. Method and provenance

| Item | Value |
|---|---|
| Tree audited | `main` @ `30353610267e7d2e36d0351c158878d2ae286d5c` = `v0.1.10^{commit}` (annotated tag `5af80b9`) |
| Version sources | `VERSION_CLI`, `src/cortex-cli/VERSION`, `[workspace.package].version` all `0.1.10`; `./target/debug/Cortex --version` → `cortex 0.1.10 (3035361 2026-09-08)` |
| PRs read | #40 (violet → banner green), #44 (intro.gif LOCK GO), #45 (bump 0.1.10); #42/#43 still open |
| Built | `cargo test --locked -p cortex-tui -p cortex-core --no-run` (exit 0, 1 m 27 s); `cargo build --locked -p cortex-cli` (exit 0) — after `apt-get install libasound2-dev libssl-dev pkg-config ripgrep` (the audio feature needs ALSA headers; README documents this) |
| Tests executed | `cargo test --locked -p cortex-tui -p cortex-core` → `cortex-core` **406 passed, 0 failed, 2 ignored**; `cortex-tui` **1 062 passed, 0 failed, 7 ignored**; doc-tests 3 passed / 108 ignored. Full `cargo test --workspace`, clippy, audit **not** run here (CI owns them). |
| Binary probes | `Cortex --help`, `Cortex mcp-server`, `Cortex debug doctor --json`, `Cortex exec --json` (loopback API), `Cortex models list` (loopback API), `Cortex mcp list --json`, TUI launch without a TTY — all in an isolated `HOME`/`CORTEX_HOME`; no live `api.cortex.foundation` request was made |
| Pixel scan | Pillow 12.3 over the 360 raw lock PNGs in `docs/media/tui-lock/{40x12,120x40}` and `docs/media/tui-lock-v2/{40x12,120x40,runtime/40x12,runtime/120x40}` (the 144 `macos/` composites were not scanned; they are 1:1 pastes of the raw pack) |
| Not observed | A live authenticated TUI session (no keyring / member credentials on this host). Anything marked *code path* below was read, not run. |

Nothing in this document is a pass rate that was not produced by a command
above. Lines are cited as `path:line` against the tree SHA in the first row.

---

## 1. Product and Designer locks vs. the tree

| Lock (from Mathis / Designer cli) | What the tree does | Verdict |
|---|---|---|
| Chrome = inky · hairlines · **green focus** (`#1F4945`, Mathis LOCK GO for intro.gif). Violet `#A78BFA` is the historical lock, **not** current. | `src/cortex-core/src/style.rs:19` `ACCENT = Rgb(31, 73, 69) // #1F4945`; `style.rs:669-719` pins it in `gray_chrome_palette_is_locked`; `lock_proof.rs:541` `ACCENT_FG = "38;2;31;73;69"`; `lock_v2.rs:1370-1378` asserts a green caret on welcome; `readme_hero.rs:215` asserts the green caret in the GIF frames. PR #40 made the switch; PR #44 re-affirmed it. | **Code = green. PASS.** |
| Same lock — evidence in the repo | Every committed PNG pack predates #40 and is violet: `docs/media/tui-lock/120x40` 65/72 files contain `#A78BFA`, 0 contain `#1F4945`; `tui-lock/40x12` 65/72; `tui-lock-v2/runtime/120x40` 73/77; `runtime/40x12` 30/31; designer boards `tui-lock-v2/120x40` 68/77, `40x12` 29/31. `login.png` (v1, 120×40) also carries 49 997 px of the retired `#221A38` wash; `thinking.png` carries 414 px of retired gold `#C9A95C`. Only `docs/media/intro.gif` + `docs/media/intro-preview/*.png` (#44) are green. | **P0 drift** — the repo's visual evidence contradicts the code; Designer cli cannot sign off against it. |
| Same lock — generator | `scripts/render-tui-lock.sh:41` `ACCENT = (167, 139, 250)` and `:53,55` fail the run unless ≥ 40 exact violet pixels exist in `splash.png` / `session_empty.png`; `:84-93` rewrites `docs/media/tui-lock/README.md` with "the Cortex violet `#A78BFA`" and "muted gold `#C9A95C`". Running the official regen script today would either fail or regress the README. | **P0 drift** |
| Same lock — tests | `lock_proof.rs:1015` `banned_colors_never_painted` still *permits* the retired `#221A38` background (`is_selection`), while `lock_v2.rs:1400-1402` bans it; `lock_proof.rs:2265-2274` `no_rounded_frame_glyphs_anywhere` renders every scene and asserts nothing (`let _ = frame;`) — a green test that proves nothing. No test anywhere bans `#A78BFA` by value. | **P1** (test debt on the lock gate) |
| Mint dead except historically banned | `style.rs:696-710` and `lock_proof.rs:1000-1012` ban mint `#00F5D4`; `readme_hero.rs:222` bans mint in the GIF. `readme_hero_boards.rs:26` doc-comment still says "`✓` mint" (comment only; the check is `SUCCESS #4ADE80`). | PASS (comment nit, P2) |
| Splash: one line `Cortex CLI vX.Y.Z` | The sign-in screen paints `Cortex CLI v{version}` as its footer / waiting header (`runner/login_screen.rs:118,416-421,454-456`; asserted at 40×12 and 120×40 by `snapshot_auth_narrow_and_wide`), and the shortcuts overlay repeats it (`widgets/shortcuts_overlay.rs:133`). The empty-session splash is two lines by lock: `Welcome to Cortex, the coding agent CLI` + `v… · / commands · @ files · ! shell · & cloud` (`views/minimal_session/rendering.rs:530-591`, `cortex-tui-components/src/welcome_card.rs`). | PASS |
| Legend stays complete at 40×12 (never truncate `@ files · ! shell`) | `splash_chrome.rs:9-38` shortens the **version**, never the legend; exact fit `v0.1.10 · / commands · @ files · ! shell` = 40 cols (`splash_chrome.rs:86-89`); `& cloud` is added only when it fits (`:73-83`). `lock_proof.rs:2341-2373` asserts the legend at both sizes. All 3 `splash_chrome` tests pass. | PASS |
| Backend = `api.cortex.foundation`; staging hop via `CORTEX_API_URL` → `:18081` | Default origin constants everywhere (`cortex-login/src/constants.rs:25`, `cortex-engine/src/client/code_agent.rs:38`, `cortex-tui/src/providers/config.rs:16`). `CORTEX_API_URL` honoured by device login (`cortex-login/src/device_api.rs:26-41`), turns (`code_agent.rs:341-343`), models (`providers/manager.rs:285`). **Exception:** `runner/app_runner/runner.rs:486` hard-codes `https://api.cortex.foundation/auth/me` and sends the stored bearer there before the TUI opens. | **P0** (staging bearer leaks to production; also `/auth/me` is not the documented `/v1/me`) |
| CLI is a coding-agent client, **Cloud default** | `code_agent.rs:72-96` `ComputerKind::detect()` defaults to `ThisPc` unless `CORTEX_COMPUTER=cloud`; `code_agent.rs:534-539` `ensure_session()` refuses to create a session for non-Cloud with *"Local and SSH Code execution require an already connected Code session…"*. Observed: `Cortex exec --json "say hi"` exits 1 with exactly that message before any network I/O. The TUI's first turn goes through the same `CodeAgentClient` (`runner/event_loop/streaming.rs:228-246`, code path). `CORTEX_COMPUTER` is documented nowhere under `docs/`. The workspace session cache advertised in `CHANGELOG.md` is dead code (`code_agent.rs:781-798`: `persist_session_id` is `#[cfg(test)]`, `cached_code_session_id` has no production caller). | **P0 — a fresh install cannot complete a turn.** |
| Chat ≠ Code ≠ Bot | Turns go to `POST /v1/code/sessions/{id}/turns` with `mode: chat\|code` and `interaction: plan\|agent` (`code_agent.rs:657-669`). Agent → `code`; Plan and Ask → `chat` + `plan` (`streaming.rs:230-235`, `app/methods.rs:696-714` maps Ask onto `OperationMode::Spec`). No Chat-clone endpoint is used. | PASS |
| Product name only | No competitor product names in UI copy (checked by grep over `src/`). But `src/cortex-tui/src/lib.rs:51,54` crate docs still say "Ocean/Cyan theme" and "Multi-provider support (…, etc.)" naming two third-party vendors; `src/cortex-cli/src/cli/args.rs:147` — the shipped `cortex --help` line 82 reads *"Model to use (e.g., …)"* with three third-party vendor model ids; `src/cortex-cli/src/login.rs:253` tells the user to pipe a third-party vendor's API-key variable; `cortex-engine/src/error.rs:402-426` suggests third-party replacement models. | **P1** (user-visible vendor names; `.rules/errors.md`, `AGENTS.md` "Do not commit") |

---

## 2. Visual-lock state matrix — what exists today

Two generations coexist. Both render headlessly through
`cortex_tui_capture::MockTerminal` (`src/cortex-tui-capture/src/mock_terminal.rs`)
at **40×12** and **120×40**.

### 2.1 Lock v1 (`docs/media/tui-lock/`, `src/cortex-tui/src/lock_proof.rs`)

* **72 scene ids** (`lock_proof.rs:53-131`), each captured at both sizes → 144
  raw frames + 144 macOS composites.
* **51** of them are *painted boards* — hand-drawn buffers in
  `lock_boards.rs` (`is_lock_board`, `lock_boards.rs:44-99`), **not** the live
  session view. This includes `splash` and `palette`.
* **4** are documented aliases of a painted board (`tool_tiles`→`grep`,
  `interrupt`→`stopped`, `compact`→`compacted`, `clear`→`clear_confirm`;
  `lock_proof.rs:211-238`, `2216-2242`).
* **17** are live widgets: the 5 login states (`LoginScreen::lock_*`,
  `login_screen.rs:145-188`) and 12 `MinimalSessionView` states
  (`palette_empty`, `settings_empty`, `session_{empty,loading,error,success}`,
  `md_{table,fence,list,mixed}`, `diff_{hunk,word}`; `lock_proof.rs:563-579`).
* Test coverage: **44** `lock_proof::tests` (all pass), incl.
  `splash_has_session_chrome`, `banner_green_is_reserved_for_selection_and_focused_composer`,
  `selection_rows_are_banner_green_on_the_selection_bar_never_inverted`,
  `green_is_reserved_for_checks_and_diff_additions`, `red_and_amber_stay_on_diagnostics`,
  `banned_colors_never_painted`, `distinct_states_render_distinct_frames`,
  `live_states_keep_chrome_complete`, `model_slugs_never_shown`,
  `lock_boards_{02_09,11_20,21_30,31_40,41_50}_product_copy`.
* Consequence: `splash_has_session_chrome` (`lock_proof.rs:2341`) proves the
  **painted** splash board, not the live splash. The live splash is proven by
  `lock_v2::tests::welcome_paints_inky_and_token_counter` and
  `views/minimal_session/tests.rs`.

### 2.2 Lock v2 (`docs/media/tui-lock-v2/`, `src/cortex-tui/src/lock_v2.rs`)

* **77 wide + 31 narrow ids** (`lock_v2.rs:33-147`), pinned by
  `lock_v2_wide_count_is_spec` (`:1317-1321`); `render-tui-lock-v2.sh:62-63`
  requires exactly 31 / 77 PNGs with unique sha256.
* **All** v2 scenes render the real widgets (`MinimalSessionView`,
  `LoginScreen`, `SettingsModalState`, `InteractiveState`) — `lock_v2.rs:208-241`.
* But **14** interactive scenes are *synthetic* `radios(...)` stand-ins
  (`lock_v2.rs:363-378`; 14 call sites) rather than the production builders /
  views: `permission-prompt` (rows at `lock_v2.rs:756-770`),
  `permission-prompt-hover`, `permissions-picker`, `plugins`, `usage`,
  `sandbox`, `sandbox-deny`, `jobs`, `question`, `sudo`, `config-tree`,
  `clear-confirm`, `plan-confirm`, `files-picker`. Five more —
  `interrupt-stopped`, `mcp-drop`, `quota-exhausted`, `diagnostics`,
  `cloud-handoff` — are `Message::system(...)` insertions, not the runtime code
  path that emits those lines.
  Real builders are used for `model-list`, `model-effort-*`, `mcp-servers`,
  `resume-picker`, `skills`, settings, login.
* Test coverage: **11** `lock_v2::tests` (all pass): uniqueness at both sizes,
  count = SPEC §7, welcome inky + token counter + green caret, slash hover is
  `#1A1A1A` and never the `#221A38` wash, effort order High→Medium→Low,
  settings modal, agent welcome copy, thought metadata, first-run tips.

### 2.3 Chrome generation the *runtime* paints

`ui/chrome.rs:127-137` paints the **v2 rounded dual-hairline composer box**
(`╭ ╮ ╰ ╯`) with mode chip and model chip; the footer is the contextual
shortcut strip (`view.rs:704-705`); `Ctrl+x` opens the shortcuts overlay and
`F2` the settings modal (`runner/event_loop/input.rs:182-208`). So SPEC v2
flags **F1** (`agent` binary — `src/cortex-cli/Cargo.toml:13-16`,
`runner.rs:978-983`), **F2** (Ctrl+x = shortcuts), **F3** (Mouse settings —
`widgets/settings_modal.rs:314-343`, wired to `tui.mouse.*` in
`cortex-engine/src/config/types.rs:395-409`) and **F4** (Appearance rows) are
resolved in code. Stale v1 statements remain: `ui/consts.rs:55-56` ("the
locked chrome never draws rounded frames"), `lock_boards.rs:2300` ("ctrl+x
clear queue"), `lock_proof.rs:2245-2261` asserting `┌ Ask — read-only ┐` square
chips on painted v1 boards while the runtime uses the `Ask · read-only` chip
(`ui/chrome.rs:112`).

### 2.4 State count for this audit

* Distinct lock ids audited: **149** (72 v1 + 77 v2), i.e. **252 frames** at
  the two lock sizes. The current Designer lock is the v2 set (77 / 31 ≥ the
  "≥ 50" target).
* Source kind: v1 — 17 live-widget ids, 51 painted boards, 4 aliases; v2 —
  63 ids from real builders / real `AppState` (5 of them seeded with
  `Message::system` stand-ins), 14 synthetic `radios()` ids.
* Pixel truth for all 360 raw PNGs: **0** files contain `#1F4945`; **330**
  contain `#A78BFA`.

The full id → source → test → proposed verifier-tool mapping is in
[`design/cli-lock-board-index.md`](../../design/cli-lock-board-index.md).

---

## 3. Surface-by-surface audit

Legend: **Works** = read + exercised by a passing test or a binary probe.
**Gap** = evidence-backed deviation with priority. Priorities: P0 blocks
"100 %", P1 must land before Designer sign-off / release, P2 hygiene.

### 3.1 Launch, splash, alternate screen

Works
* Alternate screen default with `--no-alternate-screen` / `[tui] alternate_screen` opt-out (`cli/args.rs:211-230`, `cli/handlers.rs:129-143`).
* TTY gate: no stdin/stdout TTY → product copy pointing at `cortex run` / `cortex exec` (observed).
* Splash copy + legend (see §1); first-run tips panel (`rendering.rs:570-589`); `agent` entrypoint wording (`lock_v2.rs:441-443`, `welcome_card.rs:80`).
* Trust screen tested at both sizes (4 `runner::trust_screen` tests pass).

Gaps
* **P0-1** first turn refused by default (see §1 "Cloud default").
* **P0-4** `/auth/me` to production before the TUI opens (`runner.rs:481-518`), 5 s blocking wait on the render path; also blocks startup on network I/O contrary to `.rules/tui.md:10`.
* **P2** `runner.rs:611-632` deletes stored credentials on any 401/403 from `/v1/models` during background validation — a staging 403 logs the user out of production too.

### 3.2 Sign-in (inline `LoginScreen`, `cortex login`, `/login`)

Works
* Numbered picker `Welcome to Cortex CLI!` / `How would you like to log in?` / `> 1 Continue with browser` / `· 2 Paste an API key` / hints / `Cortex CLI v…` footer at 40×12 and 120×40 (`login_screen.rs:29-36`, tests `snapshot_auth_*`, `lock_select_option_moves_the_caret_and_bar`, `login_success_check_is_the_only_green` — 6 pass).
* Device flow against `POST /v1/auth/device` + `/token` with `CORTEX_API_URL` override (`cortex-login/src/device_api.rs`); no silent guest substitution in the TUI picker (`login_screen.rs:662-691` guest path is `#[allow(dead_code)]`).
* Product copy for 401/403/404/429/5xx (`device_api.rs:237-255`).
* `cortex login --with-api-key` reads stdin only; `--token`; `login status`; `logout [--yes|--all]`.

Gaps
* **P1-1** transport failures are not product-facing: `request_device_authorization` wraps the error as *"device authorize request to {url}"* (`device_api.rs:153-159`) and both UIs display it verbatim — inline screen `login_screen.rs:801-803` (`DeviceCodeError(e.to_string())`), TUI `/login` toast `runner/event_loop/auth.rs:56,100` → `tools.rs:398-412`. Expected: *The coding service is temporarily unavailable*. The `login_error` lock frame passes only because it is fed the product string directly (`lock_proof.rs:206-208`).
* **P1-1** `cortex login --sso` prints "Starting enterprise SSO authentication..." then runs the ordinary device-code flow (`cli/handlers.rs:240-247`); `README.md:128` and `docs/reference/login.md:37-41` present it as a distinct flow.
* **P1-1** `login.rs:253` help copy names a third-party API-key variable.
* **P1-6** `CodeAgentClient::ensure_auth` (`code_agent.rs:408-425`) silently starts a guest session when no credential is found — this is the `cortex exec` path. `device_api.rs:9` and `CHANGELOG.md` say no silent guest substitution.
* **P2** `cortex whoami` (`cli/handlers.rs:565-623`) never calls `GET /v1/me`; it reports local credential state and uses `~/.cortex` directly, ignoring `CORTEX_HOME` (unlike `login.rs:36-42`). README "Check it worked with `cortex whoami`" therefore cannot detect a revoked token.
* **P2** `login_screen.rs:897-925` `begin_guest_session` (dead) uses the production constant, not `CORTEX_API_URL`.

### 3.3 Composer, slash palette, keyboard

Works
* 91 slash commands registered (`commands/registry/builtin.rs`); 21 palette-home rows in lock order with `SLASH_VISIBLE = 8` (`commands/palette_home.rs:8-36`); fuzzy match painted in accent; `… N more — keep typing to filter` trailer (`lock_proof.rs:2277-2310`).
* Composer block cursor / placeholder / blink contract (`view.rs:24-121`; `empty_composer_blink_off_drops_the_block_and_keeps_placeholder_at_col0`).
* Headless key → action → state → render harness already exists for the event loop (`runner/event_loop/ux_contract_tests.rs:10-45`: `EventLoop::new(AppState)` + `action_mapper.get_action` + `handle_action` + `MinimalSessionView` render at 40×12 and 120×40).
* `Ctrl+x` shortcuts overlay, `F2` settings (`input.rs:182-208`), `Shift+Tab` mode cycle.

Gaps
* **P1-5** `docs/reference/slash-commands.md` lacks the eight palette-home commands `/model`, `/mode`, `/permissions`, `/plan`, `/effort`, `/btw`, `/jobs`, `/interrupt` and lists `/models`, which is not registered; `/settings` alias listed as `config` (code: `prefs`). `docs/reference/keyboard.md` lacks `Ctrl+x`, `F2`, `Alt+Enter`. `docs/guides/tui.md:61` "prompt in the accent colour" (lock: past `>` is white), `:103-127` Build/Plan/Spec + `yolo/low/medium/high` autonomy (runtime: Agent/Plan/Ask chips, `/permissions` Read-only/Smart/Full access), `:152` `/models`, `:163` "**Cancelled.**" (runtime: `× Stopped`, `ui/consts.rs:41-42`).
* **P2** `commands/executor/model.rs:59-77` `/provider` deprecation copy points at `/models` (not registered).

### 3.4 Model picker vs. live `/v1/models`

Works
* `/model` → `models:fetch-and-pick` → `CortexClient::list_models` on `{api_url}/v1/models` (`providers/manager.rs:282-319`, `cortex-engine/src/client/cortex.rs:207`), then `build_model_selector` with search + effort radios Low/Medium/High and Tab (`interactive/builders/model.rs:13-55`); served slugs mapped to `Cortex Mini 1` / `Cortex 1` / `Cortex Max 1` (`ui/text_utils.rs:403-418`; `model_slugs_never_shown`).
* Startup prefetch parses `items` or `data` from the same endpoint (`runner.rs:633-652`).

Gaps
* **P1-3** the fetch is awaited **on the event loop** (`runner/event_loop/commands.rs:396-416`) — render-thread network I/O (`.rules/tui.md:10`); failure surfaces as a warning toast *"Could not fetch models, showing cached list"* and, with an empty cache, a system line *"No models available. Run /login…"* (`commands.rs:194-198`) instead of the product outage copy.
* **P1-3** `cortex models list` against an unreachable API prints only the two hint lines and exits **0** (observed with `CORTEX_API_URL=http://127.0.0.1:1`) — a silent success on failure (`AGENTS.md` "No mock-success").
* **P1-4** third-party alias table `cortex-common/src/model_presets/aliases.rs:6-60` is applied to `--model` in `cli/handlers.rs:121-124` and `acp_cmd.rs:8`, so `cortex --model sonnet` sends a third-party vendor model id to the Cortex API; `utils/model.rs:9-21` `KNOWN_PROVIDERS`; `providers/config.rs:39-45` and `modal/providers.rs:75` still ship a second provider ("Chutes (TEE)", `https://llm.chutes.ai`, `CHUTES_API_KEY`) with `validate_chutes_model` plumbing in `providers/manager.rs:14,43-52,251-275`; `--oss` flag (`args.rs:151-157`); `cortex-lmstudio` is a workspace member (`Cargo.toml:81,218`) with no in-tree consumer. `.rules/api.md`: "Do not embed a second model provider"; `.rules/security.md`: Cortex domains only.
* **P2** row copy differs from SPEC §3.9 (`Cortex Mini 1 · Fast default for everyday coding · current`): code renders `200K ctx, vision, tools` (`builders/model.rs:58-81`). Needs a Designer decision, not an invention.

### 3.5 Modes, permissions, approval prompt

Works
* `/mode`, `/permissions`, `/plan` pickers exist; `Shift+Tab` cycles Agent → Plan → Ask (`app/methods.rs:716-719`); Plan/Ask send `mode: chat`, `interaction: plan` (§1).
* Approval mode plumbing (`ApprovalMode::{Ask,AllowSession,AllowAlways}`, `app/types.rs:31-37`), `build_approval_selector` (`interactive/builders/approval.rs`), `cortex-execpolicy` + sandbox behind exec.

Gaps
* **P1-2 (Designer lock)** the runtime permission prompt is `AppView::Approval` → `views/approval.rs` — a centred modal titled *"Tool Approval Required"* with `Tool:` / `Arguments:` JSON in a `Borders::ALL` box on `SURFACE_1` (`approval.rs:70-118`, `rendering.rs:43-47`) and keys `y/n/s/a/d`. The lock (v1 `permission` board; v2 SPEC §3.10 and `permission-prompt`) is inline numbered radios under the command (`> 1 Yes, run once` …). The v2 capture never touches `ApprovalView`: `lock_v2.rs:744-771` builds synthetic `radios("Approve command", …)`. A second approval widget also exists (`widgets/approval_overlay.rs`, used by `runner/card_handler.rs:233`). The most safety-critical surface is therefore **unlocked and untested against the lock**.
* **P2** `views/approval.rs:8-10` imports legacy `BLUE, GREEN, RED` aliases.

### 3.6 Session run, streaming, cancel, errors, quota

Works
* Turn lock, SSE pump, abort-on-drop stream (`code_agent.rs:657-705`); cancel = abort + `POST /v1/code/sessions/{id}/cancel` with 3 s timeout, else `REMOTE_CANCEL_UNCONFIRMED` (`code_agent.rs:626-649`) — no fake "stopped".
* Error classification → product copy: auth → re-login flow; 402; 429/quota → `quota_held`, held composer placeholder; unavailable → *The coding service is temporarily unavailable* + next-step line (`streaming.rs:35-90`, `474-536`, `ui/consts.rs:37-53`).
* `× Stopped` recorded once (`event_loop/tests.rs:113-124`); MCP drop is a red row (`tests.rs:86-110`).
* `cortex exec` fails closed with `type: result / subtype: error / is_error: true` and non-zero exit (`tests/exec_runtime.rs`).

Gaps
* **P2** quota title is emitted as ASCII `x` (`streaming.rs:516`) while the lock glyph is `×` (`ui/consts.rs:41`; SPEC §3.14).
* **P2** `StreamErrorKind::Actionable` prints the raw error string (`streaming.rs:528`); safe only while `classify_stream_error` stays exhaustive — add a test that transport strings never reach it.
* **P2** `streaming.rs:503,508` point to `app.cortex.foundation`, a host not in the documented domain set (`.rules/docs.md`); confirm or change to `cortex.foundation/billing`.
* **P2** `tests/exec_runtime.rs` "unreachable service" runs actually fail at `ensure_session()` before any socket is opened (same message as §1); the network-failure path is untested by them.

### 3.7 MCP — client, `/mcp`, `cortex mcp`, and the missing server

Works (client side)
* Engine MCP client: stdio + streamable-HTTP transports, connection manager with lifecycle events, registry, OAuth (`cortex-engine/src/mcp/*`, 3 920 lines).
* CLI `cortex mcp {list,get,add,remove,enable,disable,rename,auth,logout,debug,tools}` (`mcp_cmd/types.rs:22-57`).
* Real-peer integration tests: `tests/exec_mcp_diagnostics.rs` drives a genuine stdio MCP peer (scripted) and proves failed initialisation → non-zero exit + JSON `connection.success=false`, unsupported protocol version rejected, empty inventory ≠ failure, config validation reasons, HTTP endpoint not listening → failed probe. `cortex mcp debug --json` already emits a machine-readable report.
* TUI `/mcp` list (`build_mcp_selector`, statuses ✓ / spinner / ×), drop detection.

Gaps
* **P0-2** there is **no verification MCP**. `cortex mcp-server` (hidden, `args.rs:366-369`) bails *"MCP server mode is not yet implemented"* (`cli/handlers.rs:35-39`; observed). The library to build it exists and is unused: `src/cortex-mcp-server` (`McpServerBuilder`, `run_stdio`, tools/resources/prompts, cancellation; 1 268 lines across `lib.rs`, `server.rs`, `builder.rs`, `handlers.rs`, `providers.rs`, with its own tests) has no consumer in the workspace (`Cargo.toml:20,177` only). Spec in §6.
* **P2** `/mcp-tools` list/call from the TUI is not covered by a headless test; `exec_mcp_diagnostics` covers list, not `tools/call`.

### 3.8 Settings, shortcuts, themes

Works
* Settings modal catalogue = SPEC §3.11 exactly (Appearance 11 rows incl. Theme submenu; Mouse 4; Behavior 6; AI 3; Git 3; Cloud 3; Privacy 2 — `widgets/settings_modal.rs:240-476`), search, theme submenu `Cortex Night / Cortex Day / Ocean Dark / Monokai`.
* Shortcuts overlay (`widgets/shortcuts_overlay.rs`), version line `Cortex CLI v{}`.

Gaps
* **P1-5** `docs/customization/themes.md:42` says the background is `Color::Reset` (runtime paints `#000000`, `ui/chrome.rs:16-27`) and `:54` lists gold `#C9A95C` as the Thinking colour (retired: `style.rs:98-99`). `docs/media/tui-lock/README.md:29` same gold claim.
* **P2** `ThemeColors::light/ocean_dark/monokai` (`style.rs:219-292`) paint cyan/pink accents; if they are shippable themes they need their own lock boards or a "Designer-unlocked" label.

### 3.9 Headless surfaces: `exec`/`run`, SDK, app-server, ACP

Works
* `cortex exec --output-format json|stream-json` with honest completion (`exec_runtime.rs`); `packages/sdk` validates the `cortex.exec.stream-json/0.1.8` frame shapes and never copies raw service errors (`packages/sdk/src/protocol.mjs`; pin documented in `docs/reference/sdk.md:10`).
* `cortex serve` / `cortex-server` local HTTP API with OpenAPI contract, auth boundary and path-escape checks exercised in CI (`scripts/readiness/qa.py:39-139`).
* `cortex acp --stdio` (JSON-RPC, stdio only, network fails closed — `acp_cmd.rs:1-53`).
* `cortex debug doctor --json` → `{ready: true, scope: local, coding_service: not_checked}` (observed) — honest about not probing the service.

Gaps
* **P0-1** applies to `exec`/`run` as well (observed).
* **P2** CI "Local functional QA" (`qa.py:26-37`) exercises `debug doctor` only; nothing in CI drives the TUI or an MCP peer end-to-end beyond the unit/snapshot jobs.

### 3.10 Hygiene the fix agents will trip over

* `src/cortex-core/src/widgets/mode_indicator.rs:53` purple `Rgb(139, 92, 246)` for `Spec` — widget has no `cortex-tui` consumer.
* `src/cortex-agents/src/agent.rs:202`, `registry.rs:79`, `src/cortex-cli/src/agent_cmd/loader.rs:172` default agent colour `#8b5cf6` (not rendered in the TUI today).
* `src/cortex-tui/src/lib.rs:51,54` crate docs (theme name, vendor list).
* Dead: `login_screen.rs:662-691` guest path; `code_agent.rs:781-830` session cache; `cortex-lmstudio` crate; `cortex-mcp-server` crate until P0-2 consumes it.
* `login_screen.rs:1002` comment still says "locked selection wash `#221A38`".

---

## 4. What already works (evidence)

* Palette is green-locked in code with contrast backing and a WCAG ≥ 4.5 test (`style.rs:648-666`); one accent, gray structure, semantic green/amber/red only.
* 149 lock ids render headlessly at 40×12 and 120×40; 55 lock tests (44 v1 + 11 v2) pass; frames are unique per state.
* Splash legend never truncates `@ files · ! shell` at 40 columns; `& cloud` appears at 120.
* Device login contract (`/v1/auth/device`, `/token`) with product copy for HTTP failures and `CORTEX_API_URL` override; keyring → encrypted-file fallback (`cortex-login`).
* Code turns: single-turn lock, SSE events (`reasoning_delta`, `text_delta`, `tool_start/end`, `usage`, `done`), cancel route, `mode`/`interaction` mapping; `exec` fails closed with machine-readable errors.
* MCP client + `cortex mcp debug/tools` with real-peer tests; TUI `/mcp` statuses; drop → red row.
* Settings modal = SPEC §3.11; Ctrl+x overlay; F2; `agent` binary alias; Mouse settings wired to config.
* Markdown auto-format proofs (`md_*`, `diff_*` scenes) render through the real `MarkdownRenderer`.
* CI gates: fmt, clippy `-D warnings`, nextest, doc-tests, schema freshness, local QA + DAST, changed-line coverage, TUI job, audit, version consistency (`.github/workflows/ci.yml`).

---

## 5. Definition of Done — "Cortex CLI works at 100 %"

All of the following, each proven by a test or by the verifier report (§6),
none by a screenshot alone:

1. **First turn works by default.** Fresh `HOME`, valid credential, `cortex` → prompt → streamed reply on **Cloud** with no env var; `cortex exec --json "…"` likewise. This PC / SSH remain explicit opt-ins.
2. **One origin.** With `CORTEX_API_URL` set, no request leaves for another host (device, `/v1/me`, `/v1/models`, sessions, turns, cancel, billing). Test: loopback fixture records every URL.
3. **Green lock everywhere.** Code, generators (`render-tui-lock.sh`, `render-tui-lock-v2.sh`, `render_lock_v2.py`), tests (ban `#A78BFA`, `#221A38`, `#C9A95C`, `#00F5D4` by value) and **regenerated** PNG packs agree on `#1F4945`; Designer cli signs the regenerated `runtime/` set.
4. **Lock parity for critical states** (permission prompt, `/permissions`, `/usage`, `/sandbox`, sandbox-deny, quota, stopped, MCP drop, diagnostics, question, plan-confirm, clear-confirm) — the v2 scene for each is produced by the **production** code path (real `AppState` mutation or real builder), not `radios()`/`Message::system` stand-ins.
5. **Product-facing errors on every path** the verifier can reach: login transport failure, `/v1/models` failure, turn failure, MCP failure, `cortex models list` failure (non-zero exit).
6. **No third-party vendor names / second provider** in help, copy, aliases, config, crates.
7. **Docs equal code** for slash commands, keyboard, TUI guide, login, themes, lock README, env vars (`CORTEX_COMPUTER`).
8. **Verifier MCP exists and is green in CI** for the offline matrix; the live matrix (models vs `/v1/models`, one Cloud turn, cancel, 429) is green when Mathis runs it with credentials against `api.cortex.foundation` and the `:18081` staging hop.

---

## 6. Verification MCP — specification (P0-2; not implemented in this PR)

### 6.1 Shape

* **Binary / entry**: implement the existing hidden `cortex mcp-server --verify` (stdio JSON-RPC) using the in-tree `cortex-mcp-server` crate (`McpServerBuilder::new("cortex-verify", VERSION_CLI)…run_stdio()`), so an agent or CI adds one MCP server entry and gets tools + resources. Keep `hide = true` until Designer sign-off; document it under `docs/guides/development.md`.
* **Driver**: headless, no PTY. Reuse `EventLoop::new(AppState)` + `ActionMapper` + `MinimalSessionView`/`ApprovalView`/pickers rendered into `cortex_tui_capture::MockTerminal` (pattern already in `runner/event_loop/ux_contract_tests.rs`). Network goes through the real `CodeAgentClient`/`CortexClient` against `CORTEX_API_URL`; **no mock model, no stub success**. Offline runs use a loopback fixture that implements the real HTTP contract (202/200 device flow, `/v1/models` JSON, SSE turn) so the verifier can assert both the happy path and the product error path; live runs are gated on `CORTEX_LIVE_API=1` and reported separately.
* **PTY optional**: a `pty.*` family (portable-pty) is a P1 extension for terminal-backend checks only (alternate screen, mouse capture, title); every chrome/state assertion must be reachable without it.

### 6.2 Tools

| Tool | Args | Returns / asserts |
|---|---|---|
| `tui.start` | `{width, height, entry: "cortex"\|"agent", resumed?: bool, api_url?, credentials: "none"\|"env"\|"keyring"}` | `session_id`, first frame sha256 |
| `tui.key` | `{session_id, keys: ["Shift+Tab", "Ctrl+x", "F2", "Esc", "Enter", "Up", …]}` | frame sha256 after each key (crossterm names via `actions/key_utils.rs::parse_key_string`) |
| `tui.type` | `{session_id, text}` | frame sha256 |
| `tui.resize` | `{session_id, width, height}` | frame sha256 (must reflow; 40×12 and 120×40 mandatory) |
| `tui.frame` | `{session_id, format: "plain"\|"ansi"\|"cells"}` | text, sha256, per-cell `{ch, fg, bg, bold}` for `cells` |
| `tui.state` | `{session_id}` | structured projection: `view`, `mode_label`, `model`, `effort`, `composer{text,caret,placeholder,focused}`, `picker{title,rows[],selected,hovered}`, `footer{left,right}`, `messages_tail[]`, `tool_rows[]`, `streaming`, `quota_held`, `mcp_servers[]`, `toasts[]` |
| `tui.assert` | `{session_id, checks: [ {kind: "contains"\|"not_contains", text}, {kind: "cell", x, y, fg?, bg?, ch?}, {kind: "no_color", rgb}, {kind: "accent_only_on_focus"}, {kind: "legend_complete"}, {kind: "row", y, eq} ]}` | `[{name, ok, detail}]`; any `ok:false` makes the state fail |
| `tui.slash` | `{session_id, query}` | palette rows `[{name, description, focused, matched_cols[]}]` |
| `tui.stop` | `{session_id}` | releases the session |
| `lock.list` | `{pack: "v1"\|"v2", width}` | scene ids (from `lock_scene_ids()` / `lock_v2_scene_ids()`) |
| `lock.render` | `{pack, id, width, height}` | plain, ansi, sha256, `{accent_px, banned_px{violet,wash,gold,mint,cyan}}` |
| `lock.diff_txt` | `{id, width, height}` | unified diff of the live grid vs `docs/media/tui-lock-v2/txt/<size>/<id>.txt` |
| `lock.palette_audit` | `{pack, width, height}` | for every id: accent only on focus glyphs, `SUCCESS` only on `✓`/`+`, red/amber only on diagnostics, no banned colours, unique frames |
| `login.run` | `{method: "browser"\|"api_key", api_url, fixture: "ok"\|"unreachable"\|"denied"\|"expired"\|"429"}` | sequence of frames (select → waiting → success/error) with the product copy asserted; `fixture` drives the loopback device endpoint |
| `api.models` | `{}` | live/fixture `GET /v1/models` ids + display names; `models_match_tui: bool` against `tui.slash("/model")` rows |
| `api.me` | `{}` | `GET /v1/me` status through the CLI client (the `whoami` DoD) |
| `api.turn` | `{session_id, message, mode: "code"\|"chat", cancel_after_ms?}` | SSE event kinds seen, `stopped_once`, cancel confirmed/unconfirmed, final frame; on 429 asserts `quota_held` + placeholder |
| `mcp.probe` | `{server}` | the `cortex mcp debug --json` report (reuse `mcp_cmd/debug.rs`) |
| `mcp.call` | `{server, tool, args}` | `CallToolResult`, duration, TUI tool row rendered |
| `report.finish` | `{run_id}` | the JSON in §6.4 written to `target/readiness/cli-verify/<run_id>.json`; non-zero exit code if any `fail` |

### 6.3 Resources

* `cortex-verify://matrix` — the state matrix (v1 ids, v2 ids, sizes, source kind live/painted/synthetic) — generated from `lock_scene_ids()`, `LOCK_V2_WIDE_IDS`, `LOCK_V2_NARROW_IDS` so it can never drift from the code.
* `cortex-verify://lock/v2/<size>/<id>.txt` — the checked-in Designer grids.
* `cortex-verify://report/latest` — last report.

### 6.4 Report (machine-readable)

```json
{
  "schema": "cortex-verify/1",
  "cli_version": "0.1.10",
  "sha": "3035361",
  "api_url": "http://127.0.0.1:18081",
  "live": false,
  "sizes": [[40, 12], [120, 40]],
  "states": [
    {"id": "welcome-cortex", "pack": "v2", "size": [40, 12], "status": "pass",
     "frame_sha256": "…", "checks": [{"name": "legend_complete", "ok": true}]}
  ],
  "flows": [
    {"id": "login.browser.unreachable", "status": "fail",
     "checks": [{"name": "product_error", "ok": false,
                 "detail": "saw 'device authorize request to http://…'"}]}
  ],
  "palette": {"accent": "#1F4945", "violet_px": 0, "wash_px": 0, "gold_px": 0, "mint_px": 0},
  "summary": {"pass": 0, "fail": 0, "skip": 0, "total": 0},
  "exit_code": 1
}
```

Rules: `skip` is never counted as `pass`; `live:false` runs must still cover
every offline state and every error flow; the CI job fails on `exit_code != 0`.

### 6.5 Minimum state coverage the verifier must assert (today's ids)

Splash/welcome (`welcome-cortex`, `welcome-agent`, `first-run-tips`,
`session-empty`), slash palette (`slash-palette`, `slash-model-typed`,
`palette_empty`), login (`login`, `login_select`, `login-waiting`,
`login-success`, `login-error`), permission prompt (`permission-prompt`,
`permission-prompt-hover`, `permissions-picker`, `sandbox-deny`), model
(`model-list`, `model-list-hover`, `model-effort-{high,medium,low,hover}` vs
`api.models`), session run (`session-thinking-live`, `shell-running`,
`tool-tiles`, `diff-hunk`, `md-table`, `code-fence`, `queue`), cancel
(`interrupt-stopped`), errors (`error-unavailable`, `quota-exhausted`,
`diagnostics`, `mcp-drop`), MCP (`mcp-servers` + `mcp.probe` + `mcp.call`),
settings (`settings-*`), shortcuts (`shortcuts-overlay`) — each at 40×12 where
the SPEC lists a narrow board, and at 120×40 always.

---

## 7. Ordered fix PRs for the coding-agent fleet (P0 → P2)

Each PR: one logical change, `type(scope): summary` subject, PR template
attestation, unit test + headless snapshot for every TUI surface touched,
`cargo fmt`, `./scripts/clippy.sh`, `cargo test --workspace`, `cargo audit`,
`./scripts/check-cli-version.sh`. No violet, no vendor names, no mock success.

### P0

**P0-1 — `fix(engine): default Code runtime to Cloud; make This PC/SSH explicit`**
* Files: `src/cortex-engine/src/client/code_agent.rs` (`ComputerKind::detect`, `ensure_session`), `src/cortex-tui/src/runner/event_loop/streaming.rs:228-246`, `src/cortex-tui/src/runner/app_runner/runner.rs` (bind cached session or create Cloud), `docs/configuration/env.md` (+`CORTEX_COMPUTER`, `CORTEX_SSH_HOST`), `docs/guides/getting-started.md`, `CHANGELOG.md`.
* Rules: `.rules/api.md` (client of the Cortex API, fail closed), product lock "Cloud default".
* Tests: engine unit test `detect()` = `Cloud` with no env; `tests/exec_runtime.rs` gains a loopback listener that records the `POST /v1/code/sessions` body `runtime:"cloud"` and then returns 503 → product error (so the "unreachable" tests really reach the socket); TUI ux-contract test: first submit reaches `stream_turn` with `computer == Cloud`. Either wire `persist_session_id`/`cached_code_session_id` into production or delete them (`.rules/structure.md` "Delete dead stubs").
* DoD 1.

**P0-2 — `feat(cli): cortex mcp-server --verify — headless TUI + API verification MCP`**
* Files: `src/cortex-cli/src/cli/handlers.rs:35-39` (replace the bail), new `src/cortex-cli/src/verify_cmd/` or `src/cortex-verify/` crate depending on `cortex-mcp-server`, `cortex-tui` (expose `lock_proof::render_lock_scene`, `lock_v2::render_lock_v2_scene`, `EventLoop` driver helpers behind a `verify` feature), `cortex-tui-capture`; `scripts/readiness/qa.py` (+ offline verifier run); `.github/workflows/ci.yml` TUI job (+ report artifact); `docs/guides/development.md`.
* Spec: §6 exactly. Tools/resources/report schema are the contract; the state matrix resource is generated from the id lists so it cannot drift.
* Tests: MCP integration test in the style of `tests/exec_mcp_diagnostics.rs` (a real stdio client drives the verifier: `initialize`, `tools/list` ≥ 20 tools, `lock.render welcome-cortex 40 12` → contains the legend, `tui.start` → `tui.slash "/"` → 8 rows + trailer, `login.run browser unreachable` → product copy, `report.finish` → schema + `exit_code`). A deliberately broken fixture (violet cell injected) must make `lock.palette_audit` fail.
* DoD 8.

**P0-3 — `fix(tui): green lock evidence — regenerate packs, fix generators and lock tests`**
* Files: `scripts/render-tui-lock.sh:37-61,84-93` (gate on `(31, 73, 69)`, README text green, thinking dim), `docs/media/tui-lock/README.md:24-30`, `docs/customization/themes.md:42,54`, regenerate `docs/media/tui-lock/{40x12,120x40,macos/**}` (v1), `docs/media/tui-lock-v2/runtime/**` (`render-tui-lock-v2.sh`), `docs/media/tui-lock-v2/{40x12,120x40}` (`tools/render_lock_v2.py --index`, palette already green), `src/cortex-tui/src/lock_proof.rs:1000-1024` (ban `#221A38` and `#A78BFA` by value at both sizes; keep `is_gray`), `lock_proof.rs:2265-2274` (assert: rounded glyphs only on the composer box rows; delete otherwise), `ui/consts.rs:55-56`, `lock_boards.rs:2300`, `readme_hero_boards.rs:26`, `login_screen.rs:1002` comment.
* Tests: Rust-side — count `Rgb(167,139,250)` / `Rgb(34,26,56)` / `Rgb(201,169,92)` cells across every v1 and v2 scene at both sizes = 0 (Pillow is **not** in `scripts/readiness/requirements.txt`, so do not make CI depend on a PNG scan; the PNG scan stays a local regen check in `render-tui-lock.sh`). `render-tui-lock-v2.sh` uniqueness gate stays.
* Designer cli signs the regenerated `runtime/` set; PR body links the before/after sha256 lists.
* DoD 3.

**P0-4 — `fix(tui): fetch /v1/me through the configured API origin`**
* Files: `src/cortex-tui/src/runner/app_runner/runner.rs:481-518` (use `CodeAgentClient`/`CortexClient` `GET {base}/v1/me`, off the render path, 3 s cap), `src/cortex-cli/src/cli/handlers.rs:565-623` (`whoami` gains a live `/v1/me` check, honours `CORTEX_HOME`).
* Rules: `.rules/security.md` (tokens never leave the configured origin), `.rules/api.md`, `.rules/tui.md:10`.
* Tests: loopback fixture asserts the only host contacted is `CORTEX_API_URL`; `whoami` against a 401 fixture prints the `cortex login` copy and exits non-zero.
* DoD 2.

### P1

**P1-1 — `fix(login): product-facing copy on every sign-in failure; --sso semantics`**
* Files: `cortex-login/src/device_api.rs:153-168` (map transport errors to `SERVICE_UNAVAILABLE`), `cortex-tui/src/runner/login_screen.rs:766-805`, `runner/event_loop/auth.rs:44-106`, `runner/event_loop/tools.rs:396-413`, `cortex-cli/src/login.rs:248-256`, `cli/handlers.rs:240-247` + `docs/reference/login.md:37-41` + `README.md:128` (either implement SSO as a distinct flow or document `--sso` as an alias and hide it).
* Tests: `login_screen` snapshot with a refused loopback endpoint shows *The coding service is temporarily unavailable* at 40×12 and 120×40; TUI `/login` toast test; `cortex login --device-auth` against loopback exits non-zero with the product line.

**P1-2 — `fix(tui): permission prompt is the locked inline radios; one approval path`**
* Files: `src/cortex-tui/src/views/approval.rs`, `widgets/approval_overlay.rs`, `runner/card_handler.rs:233`, `runner/event_loop/rendering.rs:43-47`, `app/approval.rs`, `runner/handlers/approval.rs`; `lock_v2.rs:744-799` (build `permission-prompt*` from a real `ApprovalState` on `AppState`, not `radios()`); same for `sandbox-deny`, `question`, `plan-confirm`, `clear-confirm`, `permissions-picker`, `usage`, `sandbox`, `plugins`, `jobs`, `config-tree`, `sudo`, `files-picker` (production builders/executors).
* Lock: SPEC §3.10 (`> 1 Yes, run once` … `4 No — tell Cortex what to do instead`, composer placeholder `Choose an option above`, `>` dim while the prompt owns focus).
* Tests: ux-contract test drives a real approval request → numbered rows at both sizes; keys `1-4`, `↑↓`, `Enter`, `Esc`; `lock_v2` frames unchanged in count, now from production state.
* DoD 4.

**P1-3 — `fix(tui): model picker off the render thread; honest failures; models list exit code`**
* Files: `runner/event_loop/commands.rs:396-416,172-203` (spawn fetch, `Loading…` row, product copy on failure), `src/cortex-cli/src/models_cmd.rs` (non-zero exit + product copy when `/v1/models` fails), `interactive/builders/model.rs:58-81` (SPEC §3.9 row copy once Designer confirms).
* Tests: loopback 503 → `/model` shows the outage copy, no empty picker; `cortex models list` exits non-zero.
* DoD 5.

**P1-4 — `refactor: remove second-provider and third-party model surfaces`**
* Files: `cortex-common/src/model_presets/{aliases,presets}.rs`, `cortex-cli/src/utils/model.rs`, `cortex-cli/src/cli/args.rs:147-157` (help text; drop `--oss` or make it a hidden no-op with a deprecation error), `cortex-tui/src/providers/{config,manager}.rs`, `cortex-tui/src/modal/providers.rs`, `cortex-engine/src/error.rs:402-426`, `cortex-tui/src/lib.rs:51-55`, `src/cortex-lmstudio` (+ `Cargo.toml:81,218`), `cortex-tui/src/commands/executor/model.rs:59-77`.
* Rules: `.rules/api.md`, `.rules/security.md`, `AGENTS.md` "Do not commit provider brand names".
* Tests: a workspace test greps `cortex --help`, slash descriptions and `CortexError::user_friendly_message` outputs for vendor names (list kept in the test, not in docs); `cargo machete` stays green.
* DoD 6.

**P1-5 — `docs: sync TUI docs with the lock and the registry`**
* Files: `docs/reference/slash-commands.md` (generate the table from `CommandRegistry` via a small example binary + `scripts/readiness/schema.py` entry so it can never drift), `docs/reference/keyboard.md`, `docs/guides/tui.md`, `docs/reference/login.md`, `docs/customization/themes.md`, `docs/media/tui-lock/README.md`, `docs/configuration/env.md` (`CORTEX_COMPUTER`, `CORTEX_TUI_CAPTURE`, `CORTEX_CURSOR_BLINK`).
* DoD 7.

**P1-6 — `fix(engine): no silent guest session; auth required is a product error`**
* Files: `cortex-engine/src/client/code_agent.rs:408-425`, `cortex-tui/src/runner/login_screen.rs:662-691,897-925` (delete dead guest code or route it through the explicit TUI choice).
* Tests: `exec` with no credential exits non-zero with `AUTH_REQUIRED` copy; no `POST /v1/auth/guest` observed on the loopback fixture.

### P2

**P2-1 — `chore(tui): glyph, domain and copy nits`** — `streaming.rs:516` `×`; `streaming.rs:503,508` billing host per `.rules/docs.md`; `runner.rs:611-632` do not wipe credentials on staging 403 (log + toast); `mode_indicator.rs:53`; agent default colours; `views/approval.rs:8-10` legacy aliases; `cortex mcp list --json` prints `{}` for an empty configuration (observed) — document the shape or emit `{"servers": []}`.

**P2-2 — `test(tui): make green tests prove something`** — `exec_runtime.rs` reaches a socket (after P0-1); `classify_stream_error` fuzz that transport strings never map to `Actionable`; `/mcp-tools` list + call headless test; ignored tests (`frame_engine::tests::*`, 7 `cortex-tui` ignores) get a reason review per `.rules/testing.md` ("Do not `#[ignore]` a failing test").

**P2-3 — `docs(design): Designer decisions`** — model row copy (§3.4), themes other than Cortex Night/Day (§3.8), opt-in banner legal copy (SPEC F5), timestamp locale (F9), scroll indicator trigger (F12). Record answers in `docs/media/tui-lock-v2/SPEC.md` §9.

---

## 8. Flow acceptance matrix (what the verifier asserts per flow)

| Flow | Steps (verifier tools) | Must hold at 40×12 and 120×40 |
|---|---|---|
| Cold start | `tui.start{entry:cortex}` → `tui.frame` | `Welcome to Cortex, the coding agent CLI`; legend contains `/ commands · @ files · ! shell` (`& cloud` at 120); composer `> █Plan, search, build anything`; token counter `0 / 500K`; footer `Shift+Tab:mode \| Ctrl+x:shortcuts`; accent px only on the composer `>`; zero banned colours |
| Agent entry | `tui.start{entry:agent}` | `Welcome to Cortex Agent`, placeholder `Describe a task for the agent` |
| Slash | `tui.type "/"` → `tui.slash` | 8 rows in `PALETTE_HOME_COMMANDS` order + `… N more — keep typing to filter` trailer (`view.rs:496-502`); `/mod` highlights matched chars in accent; `/zzzz` → empty state keeps composer + footer |
| Login (browser) | `login.run{method:browser, fixture:ok}` | select → `<breathing spinner> Waiting for browser authentication` + `Code: ABCD-1234` → `✓ Signed in.` (only green glyph is `✓`); `fixture:unreachable` → red *The coding service is temporarily unavailable* under the options; `fixture:denied` → `Access denied` |
| Login (API key) | `login.run{method:api_key}` | option 2 focused (`> 2 Paste an API key`), stdin-only key path, keyring/encrypted-file message |
| Model | `tui.type "/model"` → `tui.state.picker` vs `api.models` | same ids, display names `Cortex Mini 1` etc., current marked; Tab → effort radios High/Medium/Low; fixture 503 → outage copy, no empty picker |
| Modes | `tui.key Shift+Tab ×3` | chips `Agent` → `Plan · no edits` → `Ask · read-only` → `Agent`; `api.turn` body `mode/interaction` = `code/agent`, `chat/plan`, `chat/plan` |
| Session run | `tui.type "…"` `Enter` → `api.turn` | `⠇ Thinking · Ns`, `Esc:interrupt` footer, tool rows `● Shell $ …`, `✓` green only, `Worked for Ns`, `Add a follow-up — Enter to queue` placeholder while running |
| Permission | fixture emits `tool_start` needing approval | inline numbered radios under the command; composer `>` dim + `Choose an option above`; `1` runs, `4` rejects; nothing runs before the answer |
| Cancel | `api.turn{cancel_after_ms}` / `tui.key Esc` | `× Stopped` once, red; cancel POST observed or `REMOTE_CANCEL_UNCONFIRMED` shown; composer returns to idle |
| Errors | fixture 503 / 429 / 401 | 503 → outage + next-step line; 429 → `× Agent quota exhausted`, held placeholder, red bar; 401 → login flow reopened, no token in any frame |
| MCP | `mcp.probe` → `tui.type "/mcp"` → `mcp.call` | statuses ✓ / ⠇ / × match the probe; `tools/call` result renders as a tool row; kill the peer → `x <name> dropped` red row |
| Settings / shortcuts | `tui.key F2`, `/`, `scro`, `Esc`, `Ctrl+x` | catalogue rows, search filter, theme submenu; overlay opens/closes; `Cortex CLI v0.1.10` line |
| Resize | `tui.resize 40 12 ↔ 120 40` on every flow above | every assertion re-holds; no `▐` scrollbar overflow into the composer row |

---

## 9. Open questions for Mathis / Designer cli (not decided here)

1. Confirm **Cloud** is the shipped default for both TUI and `exec`, and whether This PC pairing (`/v1/code/hosts`, `code_agent.rs:617`) ships in 0.1.x or is hidden.
2. Approve the regenerated green PNG packs (P0-3) as the new signed lock; retire the violet packs or keep them under `docs/media/history/`.
3. Model row copy: SPEC §3.9 product blurbs vs the current `200K ctx, vision, tools`.
4. `--sso`: distinct WorkOS SSO flow or alias of device login.
5. Whether `light`, `ocean_dark`, `monokai` themes are Designer-locked or should be hidden until they are.

---

`CLI_100_AUDIT_READY`
