# Cortex CLI — lock board index (state → source → test → verifier tool)

Companion to [`docs/audits/CORTEX_CLI_100_AUDIT_2026-09-08.md`](../docs/audits/CORTEX_CLI_100_AUDIT_2026-09-08.md).
Generated from the id lists in the tree at `3035361` (`v0.1.10`):
`lock_scene_ids()` in `src/cortex-tui/src/lock_proof.rs:53-131`,
`is_lock_board()` in `src/cortex-tui/src/lock_boards.rs:44-99`,
`LOCK_V2_WIDE_IDS` / `LOCK_V2_NARROW_IDS` in `src/cortex-tui/src/lock_v2.rs:33-147`.

Current Designer lock: **v2** (77 wide / 31 narrow), green focus `#1F4945`.
The committed PNGs of every pack are still the historical violet render
(see audit §1); the ids and tests below are the truth the verifier reads.

Verifier tool names are the ones specified in audit §6 (`lock.render`,
`lock.palette_audit`, `lock.diff_txt`, `tui.*`, `login.run`, `api.*`,
`mcp.*`). "Flow" refers to audit §8.

## Cross-pack tests (run over every id at 40×12 and 120×40)

| Test (`cargo test -p cortex-tui`) | Pack | What it proves |
|---|---|---|
| `lock_proof::tests::banner_green_is_reserved_for_selection_and_focused_composer` | v1 | accent `#1F4945` only on focus glyphs, always on a `#F5F5F5` backing |
| `lock_proof::tests::selection_rows_are_banner_green_on_the_selection_bar_never_inverted` | v1 | no accent background; selection bar `#262626` |
| `lock_proof::tests::green_is_reserved_for_checks_and_diff_additions` | v1 | `#4ADE80` only on `✓` / `+` |
| `lock_proof::tests::every_edit_plus_count_is_green` | v1 | `+N` on Edit/Write/commit rows is diff green |
| `lock_proof::tests::red_and_amber_stay_on_diagnostics` | v1 | red/amber only on error scenes / diff deletions |
| `lock_proof::tests::banned_colors_never_painted` | v1 | mint/navy/brand-green banned — **still permits `#221A38`** (audit P0-3) |
| `lock_proof::tests::composer_is_framed_by_hairlines_in_every_session_state` | v1 | composer `>` accent + block cursor in every composer scene |
| `lock_proof::tests::distinct_states_render_distinct_frames` | v1 | only the 4 documented aliases share a frame |
| `lock_proof::tests::no_smashed_tokens_anywhere` | v1 | wrapped copy never breaks tokens |
| `lock_proof::tests::no_rounded_frame_glyphs_anywhere` | v1 | **asserts nothing** (audit P0-3) |
| `lock_v2::tests::lock_v2_wide_count_is_spec` | v2 | 77 / 31 ids |
| `lock_v2::tests::lock_v2_wide_frames_are_unique`, `lock_v2_narrow_frames_are_unique` | v2 | every id is a distinct frame |
| `lock_v2::tests::slash_hover_is_not_banner_green_wash` | v2 | hover `#1A1A1A`; `#221A38` banned |
| `style::tests::gray_chrome_palette_is_locked` (`cortex-core`) | palette | `ACCENT == #1F4945`, grays neutral, mint/cyan banned, gold retired |
| `style::tests::banner_accent_has_accessible_focus_contrast` (`cortex-core`) | palette | contrast ≥ 4.5 on the focus backing |

Verifier equivalent: `lock.palette_audit{pack, width, height}`.

## Lock v1 — 72 ids (`docs/media/tui-lock/`)

Kinds: **painted** = `lock_boards.rs` painter (not the runtime view);
**alias** = same painted board under a second id; **live** = real widget.

| Id | Kind | Specific test(s) | Verifier |
|---|---|---|---|
| `splash` | painted (`board_splash`) | `splash_has_session_chrome`, `empty_composer_blink_off_drops_the_block_and_keeps_placeholder_at_col0`, `splash_chrome::tests::*` | `lock.render v1 splash`; live equivalent = flow *Cold start* |
| `typing` | painted | `lock_boards_02_09_product_copy` | `lock.render v1 typing`; live = `tui.type` |
| `login_select` | live `LoginScreen` (option 2) | `login_is_a_numbered_picker_with_live_sub_states`, `runner::login_screen::tests::lock_select_option_moves_the_caret_and_bar` | `login.run{method:api_key}` |
| `login_waiting` | live `LoginScreen` | `login_is_a_numbered_picker_with_live_sub_states`, `snapshot_auth_waiting_and_error` | `login.run{fixture:ok}` step 2 |
| `login_success` | live `LoginScreen` | `login_success_check_is_the_only_green`, `snapshot_auth_success_and_failed` | `login.run{fixture:ok}` step 3 |
| `login_error` | live `LoginScreen` (fed the product string) | `snapshot_auth_success_and_failed` | `login.run{fixture:unreachable}` — must produce the string from the real code path (audit P1-1) |
| `palette` | painted (`board_palette`) | `slash_palette_rows_are_middot_or_banner_green_caret`, `palette_home_leads_with_lock_order` | `lock.render v1 palette`; live = flow *Slash* |
| `palette_empty` | live `MinimalSessionView` | `live_states_keep_chrome_complete` | `tui.type "/zzzz"` |
| `model_compact` | painted | `lock_boards_02_09_product_copy` | flow *Model* |
| `model_full` | painted | `model_full_is_the_full_picker_at_every_size`, `search_fields_are_framed_by_hairlines_without_a_pricing_bar` | flow *Model* |
| `mode` | painted | `pickers_are_numbered_with_dim_descriptions` | flow *Modes* |
| `permissions` | painted | `pickers_are_numbered_with_dim_descriptions` | `tui.type "/permissions"` |
| `working` | painted | `session_stays_interactive_while_running` | flow *Session run* |
| `read` | painted | `tool_tile_dots_are_white`, `tool_tiles_one_card` | flow *Session run* |
| `settings_hub` | painted | `settings_hub_is_lock_rows` | `tui.key F2` |
| `settings_empty` | live `MinimalSessionView` | `live_states_keep_chrome_complete` | `tui.key F2` → `/` → `zzzz` |
| `tool_tiles` | alias → `grep` | `tool_tiles_one_card` | — |
| `diagnostics` | painted | `diagnostics_severity_words_carry_the_only_color` | flow *Errors* |
| `multi_diff` | painted | `lock_boards_11_20_product_copy` | `tui.type "/diff"` |
| `compact` | alias → `compacted` | `compact_interrupt_clear_and_states_reflow` | — |
| `interrupt` | alias → `stopped` | `compact_interrupt_clear_and_states_reflow` | — |
| `clear` | alias → `clear_confirm` | `compact_interrupt_clear_and_states_reflow` | — |
| `session_empty` | live `MinimalSessionView` | `live_states_keep_chrome_complete`, `splash_has_session_chrome` (≠ splash) | `tui.start{resumed:true}` |
| `session_loading` | live `MinimalSessionView` | `live_states_keep_chrome_complete` | flow *Session run* (first frame) |
| `session_error` | live `MinimalSessionView` | `live_states_keep_chrome_complete` | flow *Errors* (503) |
| `session_success` | live `MinimalSessionView` | `live_states_keep_chrome_complete`, `completed_turns_do_not_get_a_fake_check` | flow *Session run* (done) |
| `shell` | painted | `green_is_reserved_for_checks_and_diff_additions` | flow *Session run* |
| `permission` | painted | `pickers_are_numbered_with_dim_descriptions` | flow *Permission* — runtime widget differs (audit P1-2) |
| `plan` | painted | `pickers_are_numbered_with_dim_descriptions` | `tui.key Shift+Tab` → plan → confirm |
| `streaming` | painted | `lock_boards_21_30_product_copy` | flow *Session run* |
| `resume` | painted | `search_fields_are_framed_by_hairlines_without_a_pricing_bar` | `tui.type "/resume"` |
| `mcp` | painted | `green_is_reserved_for_checks_and_diff_additions` | flow *MCP* |
| `usage` | painted | `lock_boards_21_30_product_copy` | `tui.type "/usage"` |
| `quota` | painted | `red_and_amber_stay_on_diagnostics` | flow *Errors* (429) |
| `sandbox` | painted | `green_is_reserved_for_checks_and_diff_additions` | `tui.type "/sandbox"` |
| `cloud` | painted | `lock_boards_31_40_product_copy` | `tui.type "& …"` |
| `sudo` | painted | `lock_boards_31_40_product_copy` | fixture: elevated Shell |
| `ask` | painted (`┌ Ask — read-only ┐` chip, v1 chrome) | `mode_chips_are_kept` | flow *Modes* — runtime chip is `Ask · read-only` (audit §2.3) |
| `files` | painted | `pickers_are_numbered_with_dim_descriptions` | `tui.type "@"` |
| `queue` | painted (footer says `ctrl+x clear queue`, stale) | `every_edit_plus_count_is_green` | flow *Session run* (queue) |
| `jobs` | painted | `green_is_reserved_for_checks_and_diff_additions` | `tui.type "/jobs"` |
| `help` | painted | `lock_boards_41_50_product_copy` | `tui.type "/help"` |
| `first_run` | painted | `first_run_tips_sit_on_the_charcoal_panel` | `tui.start{first_run:true}` |
| `bash` | painted (`┌ Bash mode ┐` chip) | `mode_chips_are_kept` | `tui.type "!"` |
| `config` | painted | `pickers_are_numbered_with_dim_descriptions` | `tui.type "/config"` |
| `footer_max` | painted | `every_edit_plus_count_is_green`, `footer_is_model_left_hint_right_and_gray` | flow *Errors* (429 → MAX) |
| `login` | live `LoginScreen` (option 1) | `login_is_a_numbered_picker_with_live_sub_states`, `snapshot_auth_select_method`, `snapshot_auth_narrow_and_wide` | `login.run` step 1 |
| `thinking` | painted (dim metadata; PNG still gold) | `lock_boards_41_50_product_copy` | flow *Session run* |
| `todos` | painted | `green_is_reserved_for_checks_and_diff_additions` | subagent fixture |
| `question` | painted | `pickers_are_numbered_with_dim_descriptions` | question-tool fixture |
| `skills` | painted | `search_fields_are_framed_by_hairlines_without_a_pricing_bar` | `tui.type "/skills"` |
| `btw` | painted | `lock_boards_41_50_product_copy` | `tui.type "/btw …"` while running |
| `stopped` | painted | `compact_interrupt_clear_and_states_reflow`, `red_and_amber_stay_on_diagnostics` | flow *Cancel* |
| `compacted` | painted | `compact_interrupt_clear_and_states_reflow` | `tui.type "/compact"` |
| `write` | painted | `every_edit_plus_count_is_green` | flow *Session run* |
| `clear_confirm` | painted | `compact_interrupt_clear_and_states_reflow` | `tui.type "/clear"` |
| `grep` | painted | `tool_tiles_one_card` | flow *Session run* |
| `glob` | painted | `lock_boards_41_50_product_copy` | flow *Session run* |
| `delete` | painted | `pickers_are_numbered_with_dim_descriptions` | `tui.type "/delete"` |
| `list` | painted | `lock_boards_41_50_product_copy` | flow *Session run* |
| `fetch` | painted | `lock_boards_41_50_product_copy` | flow *Session run* |
| `mcp_call` | painted (plus-ASCII table) | `mcp_call_issue_list_is_a_plus_ascii_table` | flow *MCP* (`mcp.call`) |
| `task` | painted | `green_is_reserved_for_checks_and_diff_additions` | subagent fixture |
| `edit` | painted | `every_edit_plus_count_is_green` | flow *Session run* |
| `md_table` | live `MinimalSessionView` | `md_table_is_a_gray_plus_ascii_grid` | fixture reply with a table |
| `md_fence` | live `MinimalSessionView` | `md_fence_has_lang_tag_line_numbers_and_hairlines` | fixture reply with a fence |
| `md_list` | live `MinimalSessionView` | `md_list_nests_bullets_and_checks_tasks` | fixture reply with lists |
| `md_mixed` | live `MinimalSessionView` | `md_mixed_is_the_auto_format_proof` | fixture reply mixed |
| `diff_hunk` | live `MinimalSessionView` | `diff_hunk_has_gutter_context_deletions_and_additions` | Edit tool fixture |
| `diff_word` | live `MinimalSessionView` | `diff_word_tints_only_the_mutated_token` | Edit tool fixture |
| `sandbox_deny` | painted | `red_and_amber_stay_on_diagnostics` | flow *Permission* (deny) |
| `mcp_drop` | painted | `red_and_amber_stay_on_diagnostics`; runtime path: `runner::event_loop::tests::mcp_disconnect_without_user_stop_is_a_drop` | flow *MCP* (kill peer) |

## Lock v2 — 77 ids (`docs/media/tui-lock-v2/`), 31 also at 40×12

Kinds: **real** = production view/builder with a real `AppState`;
**seed** = real view but the runtime-emitted line is inserted as
`Message::system`; **synthetic** = `radios()` stand-in (audit P1-2 turns these
into production state); **login** = `LoginScreen::lock_*`.

Every v2 id is covered by `lock_v2_wide_frames_are_unique` (and
`lock_v2_narrow_frames_are_unique` when narrow = yes) and by
`lock.diff_txt` against `docs/media/tui-lock-v2/txt/<size>/<id>.txt`.

| Id | Narrow | Kind | Specific test | Verifier / flow |
|---|---|---|---|---|
| `welcome-cortex` | yes | real | `welcome_paints_inky_and_token_counter`, `reported_collisions_are_distinct` | flow *Cold start* |
| `welcome-agent` | yes | real | `agent_welcome_copy` | flow *Agent entry* |
| `first-run-tips` | yes | real | `first_run_tips_are_visible` | `tui.start{first_run:true}` |
| `session-empty` | yes | real | uniqueness | `tui.start{resumed:true}` |
| `session-user-bars` | yes | real | `reported_collisions_are_distinct` | flow *Session run* |
| `session-thought` | — | real | `user_bars_and_thought_metadata` | flow *Session run* |
| `session-thought-expanded` | — | real | uniqueness | `tui.key F2` → Show thinking blocks |
| `session-thinking-live` | yes | real | uniqueness | flow *Session run* (live) |
| `session-assistant` | yes | real | `reported_collisions_are_distinct` | flow *Session run* |
| `session-worked` | — | real | uniqueness | flow *Session run* (done) |
| `session-optin` | yes | real | uniqueness | banner fixture (SPEC F5 pending) |
| `session-optin-hover` | — | real | uniqueness | mouse fixture |
| `composer-empty` | yes | real | `reported_collisions_are_distinct` | flow *Cold start* |
| `composer-typing` | yes | real | uniqueness | `tui.type` |
| `composer-typing-blink` | — | real | uniqueness | `tui.state.composer.caret_visible=false` |
| `composer-hover` | yes | real | uniqueness | mouse fixture |
| `composer-multiline` | — | real | uniqueness | `tui.key Alt+Enter` |
| `footer-shortcuts` | — | real | `reported_collisions_are_distinct` | `tui.type` (footer strip) |
| `footer-hover` | — | real | uniqueness | mouse fixture |
| `tokens-topright` | yes | real | uniqueness | `api.turn` usage event |
| `tokens-topright-warn` | — | real | uniqueness | fixture usage ≥ 90 % |
| `compact-chat` | yes | real | uniqueness | `tui.type "/compact"` |
| `slash-palette` | yes | real | `slash_hover_is_not_banner_green_wash` | flow *Slash* |
| `slash-model-typed` | yes | real | uniqueness | flow *Slash* (`/mod`) |
| `model-list` | yes | real (`build_model_selector`) | uniqueness | flow *Model* + `api.models` |
| `model-list-hover` | — | real | uniqueness | mouse fixture |
| `model-effort-high` | yes | real | `effort_order_is_high_medium_low` | flow *Model* (Tab) |
| `model-effort-medium` | — | real | uniqueness | flow *Model* |
| `model-effort-low` | — | real | uniqueness | flow *Model* |
| `model-effort-hover` | — | real | uniqueness | mouse fixture |
| `settings-appearance` | yes | real (`SettingsModalState`) | `settings_modal_has_appearance_and_search` | flow *Settings* |
| `settings-mouse` | yes | real | uniqueness | flow *Settings* |
| `settings-row-hover` | yes | real | `reported_collisions_are_distinct` | mouse fixture |
| `settings-search` | — | real | uniqueness | flow *Settings* (`/ scro`) |
| `settings-theme-submenu` | yes | real | uniqueness | flow *Settings* |
| `mode-agent` | — | real | uniqueness | flow *Modes* |
| `mode-plan` | yes | real | uniqueness | flow *Modes* |
| `mode-ask` | yes | real | uniqueness | flow *Modes* |
| `mode-bash` | — | real | uniqueness | `tui.type "!"` |
| `permission-prompt` | yes | **synthetic** | uniqueness | flow *Permission* — must come from a real approval request (P1-2) |
| `permission-prompt-hover` | — | **synthetic** | uniqueness | flow *Permission* + mouse |
| `permissions-picker` | — | **synthetic** | uniqueness | `tui.type "/permissions"` (real builder) |
| `mcp-servers` | yes | real (`build_mcp_selector`) | uniqueness | flow *MCP* (`mcp.probe`) |
| `mcp-drop` | — | seed | runtime path `mcp_disconnect_without_user_stop_is_a_drop` | flow *MCP* (kill peer) |
| `plugins` | — | **synthetic** | uniqueness | `tui.type "/plugins"` (real executor) |
| `usage` | yes | **synthetic** | uniqueness | `tui.type "/usage"` (real `billing:usage` path) |
| `quota-exhausted` | — | seed | runtime path `streaming.rs:514-523` | flow *Errors* (429) |
| `sandbox` | — | **synthetic** | uniqueness | `tui.type "/sandbox"` (`build_sandbox_selector`) |
| `sandbox-deny` | — | **synthetic** | uniqueness | flow *Permission* (deny) via `CortexError::SandboxDenied` |
| `cloud-handoff` | — | seed | uniqueness | `tui.type "& …"` |
| `diagnostics` | yes | seed | uniqueness | flow *Errors* (diagnostics tool) |
| `interrupt-stopped` | yes | seed | runtime path `interrupt_records_stopped_once` | flow *Cancel* |
| `error-unavailable` | — | real | uniqueness | flow *Errors* (503) |
| `tool-tiles` | — | real | uniqueness | flow *Session run* (Group tool calls on) |
| `tool-tiles-collapsed` | — | real | uniqueness | flow *Session run* |
| `shell-running` | — | real | uniqueness | flow *Session run* (live Shell) |
| `diff-hunk` | yes | real | uniqueness | Edit tool fixture |
| `edit-collapsed` | — | real | uniqueness | `tui.key F2` → Collapsed edit blocks |
| `md-table` | — | real | uniqueness | fixture reply with a table |
| `code-fence` | — | real | uniqueness | fixture reply with a fence |
| `login` | yes | login | `runner::login_screen::tests::*` | `login.run` step 1 |
| `login-waiting` | — | login | `snapshot_auth_waiting_and_error` | `login.run{fixture:ok}` step 2 |
| `login-success` | — | login | `login_success_check_is_the_only_green` | `login.run{fixture:ok}` step 3 |
| `login-error` | — | login (fed product string) | `snapshot_auth_success_and_failed` | `login.run{fixture:unreachable}` (P1-1) |
| `shortcuts-overlay` | yes | real | uniqueness | `tui.key Ctrl+x` |
| `resume-picker` | — | real (`build_resume_picker`) | uniqueness | `tui.type "/resume"` |
| `clear-confirm` | — | **synthetic** | uniqueness | `tui.type "/clear"` (real confirm) |
| `plan-confirm` | — | **synthetic** | uniqueness | Plan → `Implement this plan?` (real) |
| `queue` | — | real | uniqueness | flow *Session run* (Enter while running) |
| `files-picker` | — | **synthetic** | uniqueness | `tui.type "@"` (real mention picker) |
| `jobs` | — | **synthetic** | uniqueness | `tui.type "/jobs"` |
| `skills` | — | real (`build_skills_selector`) | uniqueness | `tui.type "/skills"` |
| `todos` | — | real (`SubagentTaskDisplay`) | uniqueness | subagent fixture |
| `question` | — | **synthetic** | uniqueness | question-tool fixture (`QuestionPromptView`) |
| `sudo` | — | **synthetic** | uniqueness | elevated Shell fixture |
| `config-tree` | — | **synthetic** | uniqueness | `tui.type "/config"` |
| `btw` | — | real | uniqueness | `tui.type "/btw …"` while running |

## Counts

| Pack | Ids | Sizes | Frames | Live / real | Painted or synthetic | PNGs in repo (all violet) |
|---|---|---|---|---|---|---|
| v1 | 72 | 40×12, 120×40 | 144 (+144 macOS composites) | 17 | 51 painted + 4 aliases | 65/72 files carry `#A78BFA` at each size |
| v2 | 77 wide / 31 narrow | 120×40 / 40×12 | 108 | 63 (5 seeded) | 14 synthetic | runtime 73/77 + 30/31; designer boards 68/77 + 29/31 |

Regeneration commands (after audit P0-3 lands): `./scripts/render-tui-lock.sh`,
`./scripts/render-tui-lock-v2.sh`, `python3 docs/media/tui-lock-v2/tools/render_lock_v2.py --index`.
