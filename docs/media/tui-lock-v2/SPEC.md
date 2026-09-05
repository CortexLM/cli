# Cortex CLI — TUI lock v2 · design spec

Design-only deliverable for the full TUI redesign. Layout language is taken
1:1 from the reference boards (user prompt bars, `♦ Thought` line, dual-hairline
composer with the model chip in the bottom border, Settings modal with search +
categorised rows + tip/nav footer, slash autocomplete above the composer, effort
radios under `/model`, token counter top-right, footer shortcut strip) and
re-skinned to the Cortex chrome. **No runtime code changes ship with this pack.**

- Boards: [`index.md`](index.md) — 77 boards at 120×40, 31 of them also at 40×12 (108 PNGs).
- Grids: `txt/<size>/<board>.txt` — the exact character grid of every board (diff a
  `MockTerminal` capture against these).
- Renderer: `tools/render_lock_v2.py` + `tools/boards.py` (Python 3 + Pillow, IBM Plex Mono
  fetched on first run). `python3 tools/render_lock_v2.py --index` regenerates everything.

Brand rule: the only product name on any pixel or in this document is **Cortex**
(`Cortex CLI`, `Cortex Agent`, `Cortex Cloud`, `Cortex Mini 1`, `Cortex 1`, `Cortex Max 1`,
`Cortex Night`, `Cortex Day`). No competitor names, no provider names.

---

## 1. Tokens

```json
{
  "color": {
    "bg":             "#000000",
    "text":           "#F5F5F5",
    "dim":            "#6B7280",
    "muted":          "#4B5563",
    "hairline":       "#3A3A3A",
    "hairline_hover": "#525252",
    "panel":          "#141414",
    "bar_user":       "#1C1C1C",
    "bar_hover":      "#1A1A1A",
    "bar_selected":   "#262626",
    "accent":         "#A78BFA",
    "success":        "#4ADE80",
    "warning":        "#FFC857",
    "error":          "#F87171"
  },
  "rules": {
    "accent":  "ONLY on keyboard focus: the focused `>` caret, the focused row marker/label, typed slash-command / fuzzy-matched characters",
    "success": "ONLY `✓` and diff `+N` / `+` lines",
    "warning": "ONLY warnings: diagnostics `warn`, context counter ≥ 90 %",
    "error":   "ONLY errors: `×` titles, diagnostics `error`, diff `−N` / `-` lines, failed exit codes, exhausted quota bar",
    "hairline": "always gray — never violet, never coloured",
    "bg": "inky #000 — the TUI paints the whole alternate screen; no wash, no frame",
    "retired": ["thinking gold #C9A95C", "mint #00F5D4", "cyan #7DD3FC", "violet wash #221A38 as a default bar"]
  },
  "font": {
    "family": "IBM Plex Mono",
    "weights": ["Regular", "Bold", "Italic", "BoldItalic"],
    "fallback_symbols": "DejaVu Sans Mono (♦ ▸ ▾ ▼ ● ○ ◦ ↵), Cascadia Mono (⠇ spinner)"
  }
}
```

Board geometry used by the renderer (documentation only — the product renders 1 cell = 1 terminal cell):

```json
{ "cell_px": [12, 26], "font_px": 20, "baseline_px": 20, "padding_px": 12,
  "png_120x40": [1464, 1064], "png_40x12": [504, 336] }
```

---

## 2. Viewport geometry (alternate screen, always)

```json
{
  "120x40": {
    "header_row": 0,
    "transcript_rows": [2, "composer_top - 2"],
    "composer_rows": { "top": 35, "input": 36, "bottom": 37, "grows_upward_when_multiline": true },
    "blank_row": 38,
    "footer_row": 39,
    "left_margin_col": 1,
    "right_edge_col_exclusive": 119,
    "content_col": 3,
    "bar_text_col": 5,
    "modal": { "x": 12, "y": 1, "w": 96, "h": 38 }
  },
  "40x12": {
    "header_row": 0,
    "transcript_rows": [1, 6],
    "composer_rows": { "top": 7, "input": 8, "bottom": 9 },
    "blank_row": 10,
    "footer_row": 11,
    "modal": "full screen (0,0,40,12); tip line dropped; legend compressed"
  },
  "compact_mode": { "left_margin_col": 0, "right_edge_col_exclusive": "cols", "timestamps": false,
                    "blank_rows_between_blocks": "only between turns" }
}
```

Header-left is intentionally empty on launch. It never paints a shell echo
(`~/…`, `> cortex`). Optional: the session name after `/rename`, dim.

---

## 3. Component recipes

Legend for recipes: `T` text `#F5F5F5` · `D` dim `#6B7280` · `M` muted `#4B5563` ·
`H` hairline `#3A3A3A` · `V` accent `#A78BFA` · `G` success · `A` warning · `R` error.

### 3.1 Header

```
                                                                     14K / 500K   ← D, right-aligned to col 118
```
- Format `{used} / {window}` with K/M suffixes. `0 / 500K` on welcome.
- ≥ 90 % of the window: counter turns `A` and the transcript gets one line
  `Context is 92% full — /compact summarizes the thread to free room.` (`A` + `T` + `D`).

### 3.2 User prompt bar

```
 > hey                                                                 12:49 AM    ← bar #1C1C1C, cols 1..118
```
- `>` and text `T` (never violet — history is not focus). Timestamp `D`, right-aligned inside the bar.
- 12-hour clock `hh:mm AM`. Hidden when `Show timestamps = off`, in compact mode, and at < 80 cols.
- One blank row after the bar. `&` and `!` and `/btw` prefixes stay as typed.

### 3.3 Thought line

```
   ♦ Thought for 0.4s                 (collapsed — D, glyph D)
   ♦ Thought for 3.2s ▾               (expanded: reasoning below, M, indented 2)
   ⠇ Thinking · 3s                    (live — spinner T, label D; footer shows Esc:interrupt)
```
Gold is retired. One blank row after (none in compact).

### 3.4 Reply

- Markdown through the existing renderer, `T` body, `**bold**` = Bold weight, bullets `•` / nested `◦`,
  tasks `✓`(G) / `○`(D). First line carries the timestamp `D` right-aligned; the first paragraph
  wraps `len(timestamp) + 2` columns short so it never touches it.
- Tables: gray plus-ASCII grid (`+---+`, `|` in `H`), header row Bold.
- Fences: `─ lang ─────` hairline (tag `D`), dim line-number gutter (`M`), Bold keywords, closing hairline. No background wash.
- Diff hunk: `@@` line `M`, old/new gutter `M`, context `T`, `-` lines `R`, `+` lines `G`, word-level colour only on the mutated token.

### 3.5 Worked line

```
   Worked for 4.6s                     ← D
   Worked for 12s · 4.1k tokens        ← after a stop; tokens optional
```

### 3.6 Opt-in banner (`Help improve Cortex`)

```
 Help improve Cortex                                            [Opt out]  [Opt in]
 Off by default. Opt in to let Cortex retain coding data — prompts, traces & metrics — to improve the product.
 Change anytime in /settings → Privacy.
 Read Terms and Privacy Policy.
```
- Title Bold `T`; body `D`; `Terms` / `Privacy Policy` underlined `D`.
- `[Opt in]` Bold `T` (recommended), `[Opt out]` `D`. Hover: underline + `#1A1A1A` behind the button.
  Keyboard focus: `V` label on `#262626`. Shown once per install above the composer; `▼` (D, centred)
  sits between the transcript and the banner when more content is below.

### 3.7 Composer (dual hairline, rounded)

```
 ╭─ Agent ──────────────────────────────────────────────────────────────────────╮
 │ > ▏Plan, search, build anything                                              │
 ╰──────────────────────────────────────────────────── Cortex Mini 1 (medium) ─╯
```
- Box cols 1..118, hairline `H`. Corners `╭ ╮ ╰ ╯`.
- Mode chip in the **top-left** hairline: `Agent` in `D` (default, quiet) · `Plan · no edits` in `T` ·
  `Ask · read-only` in `T` · `Bash · runs in your shell` in `T`.
- Model chip in the **bottom-right** hairline, `D`: `{display name} ({effort})`, e.g.
  `Cortex Mini 1 (medium)`, `Cortex Max 1 (high)`. Click → `/model`.
- `>` at col 3: `V` when the composer has keyboard focus, `D` otherwise (a modal, picker or prompt owns focus).
  Bash mode swaps the sigil to `!`.
- Placeholder `D` at col 5; caret (2 px bar, `T`) sits **before** the first placeholder glyph.
  Typing: text `T`, caret after the last glyph, blinks at the terminal's rate (`composer-typing` / `composer-typing-blink`).
- Slash command token in the composer is `V` (`/model`), arguments `T` (` Cortex Mini 1`); inline ghost completion `D` (`/mod` + `el`).
- Alt+Enter adds rows; the box grows upward, chips stay in place.
- Hover: every hairline of the box becomes `#525252`; nothing else changes.
- Placeholders per state: `Plan, search, build anything` (default) · `Describe a task for the agent` (agent entry) ·
  `Add a follow-up — Enter to queue` (turn running) · `Choose an option above` (prompt open) ·
  `Add a follow-up — held until quota resets` · `Reply, or ↑ to edit your last message` (after stop) ·
  `Ask about the codebase — read-only` (Ask) · `Describe what you want — Cortex drafts a plan first` (Plan).

### 3.8 Footer shortcut strip

```
 Shift+Tab:mode  |  Ctrl+x:shortcuts                                              (idle)
 Enter:send  |  Alt+Enter:newline  |  Shift+Tab:mode  |  Ctrl+x:shortcuts        (text typed)
```
- Key Bold `T`, `:label` `D`, separators `D`. Contextual sets:

```json
{
  "idle":            [["Shift+Tab","mode"],["Ctrl+x","shortcuts"]],
  "typed":           [["Enter","send"],["Alt+Enter","newline"],["Shift+Tab","mode"],["Ctrl+x","shortcuts"]],
  "typed_narrow":    [["Enter","send"],["Ctrl+x","shortcuts"]],
  "running":         [["Esc","interrupt"],["Enter","queue follow-up"],["Ctrl+x","shortcuts"]],
  "queue":           [["Enter","queue"],["↑","edit queued"],["Ctrl+c","stop"],["Ctrl+x","shortcuts"]],
  "model_list":      [["Enter","choose"],["Tab","effort"],["↑↓","select"],["Esc","close"]],
  "effort":          [["Enter","apply"],["Tab","back to models"],["Esc","close"]],
  "approval":        [["↑↓","select"],["Enter","confirm"],["e","edit command"],["Esc","cancel"]],
  "mcp":             [["Enter","details"],["r","reconnect"],["a","add server"],["Esc","close"]],
  "plugins":         [["Enter","toggle"],["i","install"],["u","update"],["Esc","close"]],
  "resume":          [["Enter","resume"],["f","favorite"],["d","delete"],["Esc","close"]],
  "bash":            [["Enter","run"],["Esc","leave bash"],["Ctrl+x","shortcuts"]],
  "unavailable":     [["Enter","retry"],["Ctrl+x","shortcuts"]]
}
```
- Hover on a chunk: key underlined, whole chunk `T` on `#1A1A1A`.

### 3.9 Menu rows (slash palette · model list · effort radios · pickers)

Stacked directly above the composer's top hairline, newest/first row on top, full-width bars (cols 1..118).

```
 > /model            Choose the model for this session            ← focused: bar #262626, `>` V, name T, desc D
   /mode             Switch between Agent, Plan and Ask           ← plain
   /permissions      Set the approval policy for edits and commands
   /plan             Draft a plan before writing any code         ← hover: bar #1A1A1A, no violet
   … 87 more — keep typing to filter                              ← trailer M
```
- Marker col 3, name col 5, description at `5 + name_w` where `name_w ≥ longest name + 2`.
- Fuzzy matching highlights **matched characters** in `V` (`/model`, `/mcp-rel**o**a**d**`, `/custo**m**-c**o**mman**d**s`).
- Slash palette shows 8 rows at 120×40 (3 at 40×12) + the `… N more` trailer.
- `/model` rows: `Cortex Mini 1 · Fast default for everyday coding · current`, `Cortex 1 · Deeper reasoning for hard changes`,
  `Cortex Max 1 · Longest context — bills by token instead of per request · MAX`.
- Effort rows (Tab from the model list, or Enter on a model): `High Effort`, `Medium Effort`, `Low Effort` with descriptions
  `Deepest reasoning — best for hard, multi-file changes` · `Balanced reasoning for everyday coding · default` ·
  `Fastest responses for quick edits and questions`. Composer reads `/model Cortex Mini 1` while the radios are open.
- Radio pickers (`/permissions`) lead with `●` (current, `T`) / `○` (`D`). Status pickers (`/mcp`, `/plugins`, `/jobs`) lead with
  `✓` G · `⠇` spinner · `×` R · `○` D.
- A one-line title sits two rows above the first row: `MCP servers · 2 of 4 connected` (`T` + `D`).

### 3.10 Inline radios (approval · confirm · question · sandbox deny)

Rendered **inside the transcript**, directly under their context — never anchored to the composer.

```
   ● Cortex wants to run
   $ npm install ioredis && npm install -D ioredis-mock                          ← command on #1C1C1C
 > 1 Yes, run once                                                              ← focused #262626, `>` V, number D
   2 Yes, always allow npm install in this project
   3 Edit command
   4 No — tell Cortex what to do instead
```
Composer loses focus (`>` `D`, placeholder `Choose an option above`). Hover row `#1A1A1A`.

### 3.11 Settings modal (F2 · `/settings`)

```
 ╭─ Settings ──────────────────────────────────────────────────────────────── [x] ─╮
 │  / to search                                                                    │
 │  ───────────────────────────────────────────────────────────────────────────────│
 │                                                                                 │
 │  Appearance ────────────────────────────────────────────────────────────────    │
 │   ▸ Compact mode                                                        off     │  ← focused: bar, ▸ V, label Bold
 │   ▸ Default screen mode                                        Fullscreen  >    │  ← `>` = submenu
 │   ▸ Show timestamps                                                      on     │
 │  …                                                                              │
 │        Tip · Ask Cortex: "change theme to Cortex Day" or "what does compact mode do?"   │
 │  ↑/↓/j/k nav | g/G top/btm | Space toggle | Enter toggle | → expand | / search | d reset │
 │                                  F2/Esc close                                   │
 ╰─────────────────────────────────────────────────────────────────────────────────╯
```
- Values: `on` `T`, `off` `D`, numbers `T`, submenu value `T` + `  >` `D`. Category header `D` + hairline to the right edge.
- Search: `/` focuses the field (`/ ` turns `V`), rows filter live, matched substring `V`, categories without matches disappear.
- Submenu (→ / Enter on a `>` row): breadcrumb `Appearance › Theme`, radio rows `●`/`○`, focused label `V` Bold; legend `Enter select | ← back`.
- Row hover: `#1A1A1A` bar, marker stays `D`. Focus + hover: focus wins.
- Categories and rows (see §7 for which rows are new):

```json
{
  "Appearance": [["Compact mode","off"],["Default screen mode","Fullscreen >"],["Show timestamps","on"],["Show thinking blocks","on"],["Group tool calls","on"],["Collapsed edit blocks","off"],["Line numbers","on"],["Word wrap","on"],["Syntax highlight","on"],["Animations","on"],["Theme","Cortex Night >"]],
  "Mouse":      [["Mouse capture","on"],["Scroll lines","3"],["Invert scroll","off"],["Copy on select","off"]],
  "Behavior":   [["Auto approve","off"],["Sandbox mode","on"],["Streaming","on"],["Auto scroll","on"],["Sound","off"],["Notifications","off"]],
  "AI":         [["Thinking mode","on"],["Context aware","on"],["Debug mode","off"]],
  "Git":        [["Co-author","on"],["Auto commit","off"],["Sign commits","off"]],
  "Cloud":      [["Cloud sync","off"],["Auto save","on"],["Session history","on"]],
  "Privacy":    [["Telemetry","off"],["Analytics","off"]],
  "Theme submenu": [["Cortex Night","Default inky chrome · violet on focus only","current"],["Cortex Day","Light chrome for bright rooms"],["Ocean Dark","Deep blue and cyan accents"],["Monokai","Classic code-editor colors"]]
}
```

### 3.12 Shortcuts overlay (Ctrl+x)

Same modal chrome, 84×14, two columns of `key` (`T`, padded) + `label` (`D`); a hairline, then
`Docs & guides: cortex.foundation/docs · Cortex CLI v0.1.7` (`M`); legend `Ctrl+x/Esc close`.

### 3.13 Tool tiles

```
   ● Shell  $ cargo test -p cortex-tui · ✓ 0 · 41s          ● D · tool Bold T · arg T · meta D · ✓ G / × R
   ● Read   src/cortex-tui/src/composer.rs · 212 lines
   ● Grep   "alternate_screen" in src/ · 6 hits in 4 files
   ● Edit   src/cortex-tui/src/composer.rs · +12 −3          +N G · −N R · `▸ show diff` D when collapsed
   ⠇ Shell  $ cargo test -p cortex-tui · running · 8s        live: spinner T, output lines D indented 2
   ▾ 3 tool calls  Read ×2 · Grep ×1                          grouped (Group tool calls = on); ▸ when collapsed
   ● Diagnostics  src/…/composer.rs · 2
      error  E0308 mismatched types — …   composer.rs:42:9   error R · warn A · location D
   × Stopped                                                 R, then `Worked for 12s · 4.1k tokens` D
   × The coding service is temporarily unavailable           R, then `Try again in a moment — …` D
   ↑ Handed off to Cortex Cloud                              T; `agent` / `branch` / `follow` keys D, values T
```

### 3.14 Usage / quota

`Usage · Cortex Pro · renews Oct 1` then rows `label (T) · used/total (D) · 40-cell bar (filled `█` D, empty `█` #262626) · pct (D)`.
Exhausted: `× Agent quota exhausted` R, bar filled R, composer placeholder `Add a follow-up — held until quota resets`.
Narrow: no bars, `used / total  pct%`.

### 3.15 Login (`cortex login`, inline — no alternate screen)

`Welcome to Cortex CLI!` Bold → `How would you like to log in?` → radios `1 Continue with browser · Opens cortex.foundation/cli/auth`,
`2 Paste an API key · Enter your key to authenticate` → waiting (`⠇ Waiting for browser authentication…`, `Your code  WXYZ-1234` Bold)
→ `✓ Signed in as …` (✓ G) or `× Sign-in didn't complete` (R) + product copy.

---

## 4. Hover vs keyboard focus

```json
{
  "row (settings, slash, model, effort, radios, pickers)": {
    "idle":  { "bar": "none",    "marker": "none / ▸ dim", "label": "text" },
    "hover": { "bar": "#1A1A1A", "marker": "unchanged",    "label": "text", "accent": false },
    "focus": { "bar": "#262626", "marker": "> or ▸ in #A78BFA", "label": "text (Bold in settings)", "accent": true },
    "focus+hover": "focus wins"
  },
  "composer": {
    "idle":  { "hairline": "#3A3A3A", "caret_sigil": "#6B7280" },
    "hover": { "hairline": "#525252", "caret_sigil": "unchanged" },
    "focus": { "hairline": "#3A3A3A", "caret_sigil": "#A78BFA", "caret": "2 px bar, blinking" }
  },
  "footer chunk / banner button / chip": {
    "hover": { "underline": true, "text": "#F5F5F5", "bar": "#1A1A1A" },
    "focus": { "text": "#A78BFA", "bar": "#262626" }
  },
  "typed slash match": { "accent": true, "note": "matched characters in the palette and the command token in the composer" }
}
```

---

## 5. Welcome copy

```json
{
  "cortex": {
    "line1": "Welcome to **Cortex**, the coding agent CLI",
    "line2": "v0.1.7  ·  / commands  ·  @ files  ·  ! shell  ·  & cloud",
    "placeholder": "Plan, search, build anything",
    "header": "0 / 500K"
  },
  "agent": {
    "line1": "Welcome to **Cortex Agent** — describe a task, it does the work",
    "line2": "v0.1.7  ·  / commands  ·  @ files  ·  ! shell  ·  & cloud",
    "placeholder": "Describe a task for the agent",
    "header": "0 / 500K"
  },
  "narrow": { "line1": "Welcome to **Cortex**", "line2": "v0.1.7 · / commands · @ files" },
  "first_run_panel": {
    "bg": "#141414",
    "title": "A few tips to get the most out of this tool:",
    "tips": [
      "1. Use /model to switch between models and adjust reasoning effort.",
      "2. Add @ files to give Cortex the right context.",
      "3. Press Shift+Tab anytime to cycle Agent / Plan / Ask.",
      "4. Ctrl+x lists every shortcut · F2 opens settings."
    ]
  },
  "never_paint": ["~/…", "> cortex", "$ prompt echoes", "ASCII logo"]
}
```

---

## 6. Reference → Cortex rename table

| Reference element | Cortex |
|---|---|
| Product / assistant name in replies | **Cortex** |
| Model chip `<vendor model> (high)` | `Cortex Mini 1 (medium)` · `Cortex 1 (high)` · `Cortex Max 1 (high)` |
| `/model <vendor model>` argument | `/model Cortex Mini 1` |
| Themes `<vendor> Night` / `<vendor> Day` | `Cortex Night` / `Cortex Day` (config ids `dark` / `light` unchanged) |
| Settings tip `Ask <vendor>: "change theme to …"` | `Ask Cortex: "change theme to Cortex Day" or "what does compact mode do?"` |
| `Help improve <vendor>` / `<company> to retain coding data` | `Help improve Cortex` / `let Cortex retain coding data` |
| `/resume-<other tool>`, `/import-<other tool>` | not carried over — Cortex has `/resume` (own sessions) only |
| `/compact-mode`, `/vim-mode` | `/compact` (existing); vim keys are a navigation style, no slash command |
| "Thought for Xs" · "Worked for Xs" | kept verbatim (new strings for Cortex) |
| `Shift+Tab:mode | Ctrl+x:shortcuts` | kept verbatim |
| Effort `High Effort / Medium Effort / Low Effort` | kept; descriptions rewritten (see §3.9) |
| Caret `|` before placeholder | 2 px bar caret before the placeholder |
| `~` cwd in the header | removed (header-left empty) |

---

## 7. State checklist

| Requested state | Board(s) |
|---|---|
| welcome-cortex / welcome-agent / session-empty | `welcome-cortex`, `welcome-agent`, `session-empty` (+ `first-run-tips`) |
| session-user-bars / thought / assistant / worked / optin | `session-user-bars`, `session-thought`, `session-thought-expanded`, `session-thinking-live`, `session-assistant`, `session-worked`, `session-optin`, `session-optin-hover` |
| composer-empty / typing (+blink) / hover | `composer-empty`, `composer-typing`, `composer-typing-blink`, `composer-hover`, `composer-multiline` |
| footer-shortcuts / tokens-topright | `footer-shortcuts`, `footer-hover`, `tokens-topright`, `tokens-topright-warn` |
| slash-palette / slash-model-typed | `slash-palette`, `slash-model-typed` |
| model-effort-low·medium·high / model-list-hover | `model-list`, `model-list-hover`, `model-effort-high`, `model-effort-medium`, `model-effort-low`, `model-effort-hover` |
| settings-appearance / mouse / row-hover / search / theme-submenu | `settings-appearance`, `settings-mouse`, `settings-row-hover`, `settings-search`, `settings-theme-submenu` |
| mode chips (Shift+Tab) | `mode-agent`, `mode-plan`, `mode-ask`, `mode-bash` |
| permissions / mcp / plugins / usage-quota / sandbox / cloud | `permission-prompt`, `permission-prompt-hover`, `permissions-picker`, `mcp-servers`, `mcp-drop`, `plugins`, `usage`, `quota-exhausted`, `sandbox`, `sandbox-deny`, `cloud-handoff` |
| diagnostics red/amber · interrupt/stopped | `diagnostics`, `interrupt-stopped`, `error-unavailable` |
| markdown table · diff hunk · code fence | `md-table`, `diff-hunk`, `edit-collapsed`, `code-fence`, `tool-tiles`, `tool-tiles-collapsed`, `shell-running` |
| login / first-run | `login`, `login-waiting`, `login-success`, `login-error`, `first-run-tips` |
| compact chat (ref 5) | `compact-chat` |
| other product surfaces (not in the brief, kept complete) | `shortcuts-overlay`, `resume-picker`, `clear-confirm`, `plan-confirm`, `queue`, `files-picker`, `jobs`, `skills`, `todos`, `question`, `sudo`, `config-tree`, `btw` |

Narrow (40×12) set: `welcome-cortex`, `welcome-agent`, `first-run-tips`, `session-empty`, `session-user-bars`,
`session-thinking-live`, `session-assistant`, `session-optin`, `composer-empty`, `composer-typing`, `composer-hover`,
`tokens-topright`, `compact-chat`, `slash-palette`, `slash-model-typed`, `model-list`, `model-effort-high`,
`settings-appearance`, `settings-mouse`, `settings-row-hover`, `settings-theme-submenu`, `mode-plan`, `mode-ask`,
`permission-prompt`, `mcp-servers`, `usage`, `diagnostics`, `interrupt-stopped`, `diff-hunk`, `login`, `shortcuts-overlay`.

---

## 8. Keybinding contract (as drawn)

```json
{
  "Shift+Tab": "cycle Agent → Plan → Ask (chip in the composer top hairline)",
  "Ctrl+x":    "shortcuts overlay (was: clear follow-up queue — see flag F2)",
  "F2":        "settings modal (also /settings)",
  "Esc":       "interrupt the turn · close modal/picker · leave bash mode",
  "Enter":     "send · confirm · queue a follow-up while running",
  "Alt+Enter": "newline in the composer",
  "Tab":       "complete slash command · jump between model list and effort radios",
  "/":         "slash palette (in the composer) · search (in Settings)",
  "@": "file picker", "!": "bash mode", "&": "hand off to Cortex Cloud",
  "↑": "edit last / edit queued", "Ctrl+c": "stop", "Ctrl+p": "command palette", "Ctrl+r": "search past sessions",
  "settings": { "↑/↓/j/k": "nav", "g/G": "top/bottom", "Space/Enter": "toggle", "→": "expand submenu", "←": "back", "d": "reset row", "F2/Esc": "close" }
}
```

---

## 9. Flags for Mathis (states I could not invent safely)

```json
[
  {"id":"F1","area":"welcome-agent","flag":"There is no `agent` binary or alias in `src/cortex-cli/Cargo.toml` (only `[[bin]] name = \"Cortex\"`, clap name `cortex`). The board assumes an `agent` entrypoint that launches the same TUI; the only differences drawn are the title line and the placeholder. Confirm the alias exists / is planned and the wording."},
  {"id":"F2","area":"footer","flag":"`Ctrl+x` is drawn as `shortcuts` (1:1 with the reference). In the current runtime `Ctrl+x` clears the follow-up queue. Proposal: queue items are removed with `↑` (edit) then Backspace/`d` on the queued row; the `queue` footer reads `Enter:queue | ↑:edit queued | Ctrl+c:stop | Ctrl+x:shortcuts`. Needs a decision."},
  {"id":"F3","area":"settings › Mouse","flag":"`Mouse capture`, `Scroll lines`, `Invert scroll`, `Copy on select` do not exist in `TuiConfig` (only `animations`, `alternate_screen`, `notifications`, `theme`). Drawn because the brief asks for a Mouse section; new config keys (`tui.mouse.*`) required."},
  {"id":"F4","area":"settings › Appearance","flag":"`Show thinking blocks`, `Group tool calls`, `Collapsed edit blocks`, `Show timestamps`, `Default screen mode` map to: SettingsSnapshot `Thinking Mode` / new / new / `Timestamps` / `tui.alternate_screen` (Fullscreen = true, Inline = false). Group/collapse are new behaviours."},
  {"id":"F5","area":"opt-in banner","flag":"No `Help improve Cortex` banner exists today (telemetry is a Privacy toggle, default off). Copy is a draft; `Terms` / `Privacy Policy` need real URLs and legal sign-off."},
  {"id":"F6","area":"strings","flag":"`♦ Thought for Xs` and `Worked for Xs` replace `Thinking` (gold) and `1m 12s · 8.2k tokens`. Gold `#C9A95C` is retired because amber-ish colour is reserved for warnings."},
  {"id":"F7","area":"themes","flag":"`dark` → `Cortex Night`, `light` → `Cortex Day` are display renames; keep the config ids. `Ocean Dark` / `Monokai` kept as-is. The theme modal description `Default dark theme with green accents` is wrong for this chrome and should read like the submenu board."},
  {"id":"F8","area":"effort order","flag":"Radios are drawn High → Medium → Low (reference order). The current horizontal radios read Low · Medium · High. Pick one; Tab from the model list lands on the current effort."},
  {"id":"F9","area":"timestamps","flag":"12-hour `hh:mm AM` clock as in the reference. Locale-aware 24 h is an open choice."},
  {"id":"F10","area":"plugins","flag":"Plugin names on the `plugins` board (`cortex-review`, `mermaid-preview`, `jira`) are illustrative; the real registry copy is unknown."},
  {"id":"F11","area":"header-left","flag":"Left side of row 0 is empty by design (no cwd echo). Optional session name after `/rename` — not drawn."},
  {"id":"F12","area":"scroll indicator","flag":"`▼` centred above the banner is drawn as in the reference when content continues below; exact trigger (scrolled up vs new output) is an implementation choice."}
]
```

---

## 10. Implementation notes for the runtime PR (not part of this delivery)

- Chrome constants live in `src/cortex-core/src/style.rs`; add `bar_hover #1A1A1A`, retire gold, keep the rest of §1.
- Composer: model chip moves from the footer (`Cortex Mini 1 · Agent · 92% context`) into the bottom hairline; mode moves into the
  top hairline; context moves to the header. The footer becomes the contextual shortcut strip (§3.8).
- Menus reuse the existing picker rows (`palette_home.rs`, `/model`, `/permissions`, `/mcp`) with the bar/marker recipe of §3.9;
  approval / plan / clear / question prompts render inline (§3.10) using the same recipe.
- Settings hub (7 rows) is replaced by the categorised modal (§3.11); `/settings` and F2 open it.
- Every changed surface needs a unit test plus a headless snapshot; the `txt/` grids in this pack are the expected buffers.
