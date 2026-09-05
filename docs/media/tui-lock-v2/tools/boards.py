"""Board recipes for the Cortex CLI TUI lock v2.

Every board is a function ``fn(screen, ctx)`` that paints a full viewport
(alternate screen, default launch). The shared chrome helpers at the top are
the component recipes documented in ``../SPEC.md`` — keep them in sync.

Layout language (from the reference boards, re-skinned to the Cortex chrome):

* row 0            header — session name (optional, dim) left · token counter right
* rows 2..         transcript — user prompt bars, ``♦ Thought for Xs``, replies,
                   ``Worked for Xs``, tool tiles, panels
* rows H-5..H-3    composer — rounded hairline box, mode chip in the top border
                   (left), model chip in the bottom border (right)
* row H-1          footer — shortcut strip ``Key:label | Key:label``

Focus vs hover: keyboard focus = ``#262626`` bar + violet marker/caret;
mouse hover = ``#161616`` bar (or ``#525252`` hairline) and *no* violet.
"""

from __future__ import annotations

import textwrap
from dataclasses import dataclass

from render_lock_v2 import (
    AMBER,
    BAR_HOV,
    BAR_SEL,
    BAR_USER,
    BG,
    DIM,
    GREEN,
    HAIR,
    HAIR_HI,
    MUTED,
    PANEL,
    RED,
    S,
    S_ACC,
    S_BOLD,
    S_DIM,
    S_ERR,
    S_HAIR,
    S_MUTED,
    S_OK,
    S_WARN,
    TEXT,
    VIOLET,
    Cell,
    Screen,
    St,
    clip,
)

VERSION = "0.1.7"
MODEL_CHIP = "Cortex Mini 1 (medium)"
PLACEHOLDER = "Plan, search, build anything"
TOKENS = "14K / 500K"


@dataclass
class Ctx:
    cols: int
    rows: int
    narrow: bool

    @property
    def x0(self) -> int:
        return 1

    @property
    def x1(self) -> int:  # exclusive right edge of bars / boxes
        return self.cols - 1

    @property
    def inner_w(self) -> int:
        return self.x1 - self.x0


# --------------------------------------------------------------------------- #
# Rich text helpers
# --------------------------------------------------------------------------- #


def parse_markup(text: str, base: St = S) -> list:
    """``**bold**`` → bold spans, ``__u__`` → underline spans, `` `code` `` → text."""
    parts, buf, i = [], "", 0
    bold, under = False, False
    while i < len(text):
        if text.startswith("**", i):
            if buf:
                parts.append((buf, base(b=bold, u=under)))
                buf = ""
            bold = not bold
            i += 2
        elif text.startswith("__", i):
            if buf:
                parts.append((buf, base(b=bold, u=under)))
                buf = ""
            under = not under
            i += 2
        else:
            buf += text[i]
            i += 1
    if buf:
        parts.append((buf, base(b=bold, u=under)))
    return parts


def wrap_markup(text: str, width: int, base: St = S) -> list:
    """Greedy word-wrap of a markup string; returns a list of span lists."""
    words = []  # (word, style)
    for chunk, st in parse_markup(text, base):
        pieces = chunk.split(" ")
        for k, piece in enumerate(pieces):
            if k > 0:
                words.append((" ", st))
            if piece:
                words.append((piece, st))
    lines, cur, cur_len = [], [], 0
    for word, st in words:
        if word == " ":
            if cur:
                cur.append((word, st))
                cur_len += 1
            continue
        if cur_len + len(word) > width and cur:
            while cur and cur[-1][0] == " ":
                cur.pop()
            lines.append(cur)
            cur, cur_len = [], 0
        cur.append((word, st))
        cur_len += len(word)
    while cur and cur[-1][0] == " ":
        cur.pop()
    if cur:
        lines.append(cur)
    return lines or [[]]


# --------------------------------------------------------------------------- #
# Chrome recipes
# --------------------------------------------------------------------------- #


def header(s: Screen, c: Ctx, tokens: str = TOKENS, left: str | None = None, warn: bool = False):
    if left:
        s.put(c.x0, 0, clip(left, c.inner_w - len(tokens) - 2), S_DIM)
    s.right(0, tokens, c.x1, S_WARN if warn else S_DIM)


FOOTER_IDLE = [("Shift+Tab", "mode"), ("Ctrl+x", "shortcuts")]
FOOTER_TYPED = [("Enter", "send"), ("Alt+Enter", "newline"), ("Shift+Tab", "mode"), ("Ctrl+x", "shortcuts")]
FOOTER_TYPED_NARROW = [("Enter", "send"), ("Ctrl+x", "shortcuts")]


def footer(s: Screen, c: Ctx, hints=None, hover: str | None = None, y: int | None = None):
    hints = hints or FOOTER_IDLE
    y = c.rows - 1 if y is None else y
    x = c.x0
    for k, (key, label) in enumerate(hints):
        if k:
            x = s.put(x, y, "  |  " if not c.narrow else " | ", S_DIM)
        chunk = f"{key}:{label}"
        if x + len(chunk) > c.x1:
            break
        if hover == key:
            s.fill(x - 1, y, x + len(chunk) + 1, BAR_HOV)
            x = s.put(x, y, key, St(fg=TEXT, b=True, u=True, bg=BAR_HOV))
            x = s.put(x, y, ":" + label, St(fg=TEXT, bg=BAR_HOV))
        else:
            x = s.put(x, y, key, St(fg=TEXT, b=True))
            x = s.put(x, y, ":" + label, S_DIM)


MODE_CHIPS = {
    "Agent": (" Agent ", S_DIM),
    "Plan": (" Plan · no edits ", S),
    "Ask": (" Ask · read-only ", S),
    "Bash": (" Bash · runs in your shell ", S),
}


def composer(
    s: Screen,
    c: Ctx,
    content=None,
    placeholder: str = PLACEHOLDER,
    model: str = MODEL_CHIP,
    mode: str = "Agent",
    focused: bool = True,
    hover: bool = False,
    caret: bool = True,
    extra_lines: int = 0,
    sigil: str = ">",
    y_bottom: int | None = None,
) -> int:
    """Paint the composer box; returns the row of its top border."""
    hair = HAIR_HI if hover else HAIR
    bottom = (c.rows - 3) if y_bottom is None else y_bottom
    inner_rows = 1 + extra_lines
    top = bottom - inner_rows - 1
    s.box(c.x0, top, c.inner_w, inner_rows + 2, fg=hair)
    # mode chip, top-left, inside the hairline
    chip, chip_st = MODE_CHIPS.get(mode, (f" {mode} ", S))
    if c.narrow and mode == "Agent":
        chip = " Agent "
    s.put(c.x0 + 2, top, clip(chip, c.inner_w - 6), chip_st)
    # model chip, bottom-right, inside the hairline
    m = f" {model} "
    s.right(bottom, m, c.x1 - 2, S_DIM)
    # prompt
    y = top + 1
    s.put(c.x0 + 2, y, sigil, S_ACC if focused else S_DIM)
    tx = c.x0 + 4
    max_x = c.x1 - 2
    if content:
        x = s.spans(tx, y, content, max_x=max_x)
        if caret:
            s.caret = (min(x, max_x), y)
    else:
        s.put(tx, y, clip(placeholder, max_x - tx), S_DIM)
        if caret:
            s.caret = (tx, y)
    return top


def scroll_indicator(s: Screen, c: Ctx, y: int):
    s.center(y, "▼", c.x0, c.x1, S_DIM)


def optin_banner(s: Screen, c: Ctx, y: int, hover: str | None = None, focus: str | None = None) -> int:
    """`Help improve Cortex` block above the composer. Returns rows used."""
    s.put(c.x0, y, "Help improve Cortex", S_BOLD)
    out_st = S_DIM
    in_st = S_BOLD
    if hover == "in":
        in_st = St(fg=TEXT, b=True, u=True, bg=BAR_HOV)
    if hover == "out":
        out_st = St(fg=TEXT, u=True, bg=BAR_HOV)
    if focus == "in":
        in_st = St(fg=VIOLET, b=True, bg=BAR_SEL)
    if focus == "out":
        out_st = St(fg=VIOLET, b=True, bg=BAR_SEL)
    s.right_spans(0 + y, [("[Opt out]", out_st), ("  ", S), ("[Opt in]", in_st)], c.x1)
    if c.narrow:
        s.put(c.x0, y + 1, "Off by default · anytime in /settings", S_DIM)
        s.spans(c.x0, y + 2, [("Read ", S_DIM), ("Terms", S_DIM(u=True)), (" and ", S_DIM), ("Privacy Policy", S_DIM(u=True)), (".", S_DIM)])
        return 3
    s.put(
        c.x0,
        y + 1,
        clip("Off by default. Opt in to let Cortex retain coding data — prompts, traces & metrics — to improve the product.", c.inner_w),
        S_DIM,
    )
    s.put(c.x0, y + 2, "Change anytime in /settings → Privacy.", S_DIM)
    s.spans(c.x0, y + 3, [("Read ", S_DIM), ("Terms", S_DIM(u=True)), (" and ", S_DIM), ("Privacy Policy", S_DIM(u=True)), (".", S_DIM)])
    return 4


class Flow:
    """Sequential transcript painter. ``compact`` drops margins + timestamps."""

    def __init__(self, s: Screen, c: Ctx, y: int = 2, limit: int | None = None, compact: bool = False):
        self.s, self.c, self.y = s, c, y
        self.limit = limit if limit is not None else c.rows - 6
        self.compact = compact
        self.x0 = 0 if compact else c.x0
        self.x1 = c.cols if compact else c.x1
        self.tx = self.x0 + 2  # content column (`>`/`♦` column)

    def ok(self, n: int = 1) -> bool:
        return self.y + n <= self.limit

    def blank(self, n: int = 1):
        self.y += n

    def stamp(self, y: int, time: str | None):
        if time and not self.compact and not self.c.narrow:
            self.s.right(y, time, self.x1 - 1, S_DIM)

    def user(self, text: str, time: str | None = None, sigil: str = ">"):
        if not self.ok():
            return
        self.s.fill(self.x0, self.y, self.x1, BAR_USER)
        bar = St(fg=TEXT, bg=BAR_USER)
        self.s.put(self.tx, self.y, sigil, bar)
        room = self.x1 - 1 - (self.tx + 2) - (len(time) + 2 if (time and not self.compact and not self.c.narrow) else 0)
        self.s.put(self.tx + 2, self.y, clip(text, room), bar)
        self.stamp(self.y, time)
        self.y += 1
        if not self.compact:
            self.y += 1

    def thought(self, label: str = "Thought for 0.4s", live: bool = False):
        if not self.ok():
            return
        glyph = "⠇" if live else "♦"
        self.s.spans(self.tx, self.y, [(glyph, S_DIM), (" ", S), (label, S_DIM)])
        self.y += 1
        if not self.compact:
            self.y += 1

    def thought_body(self, lines):
        for ln in lines:
            if not self.ok():
                return
            self.s.put(self.tx + 2, self.y, clip(ln, self.x1 - 1 - self.tx - 2), S_MUTED)
            self.y += 1
        if not self.compact:
            self.y += 1

    def reply(self, paragraphs, time: str | None = None):
        """paragraphs: list of markup strings ('' = blank line, '• ' bullets)."""
        width = self.x1 - 1 - self.tx
        first = True
        for para in paragraphs:
            if para == "":
                self.y += 1
                continue
            # the first paragraph shares its row with the timestamp — keep clear of it
            w = width - (len(time) + 2) if (first and time and not self.compact and not self.c.narrow) else width
            for line in wrap_markup(para, w):
                if not self.ok():
                    return
                self.s.spans(self.tx, self.y, line, max_x=self.x1 - 1)
                if first:
                    self.stamp(self.y, time)
                    first = False
                self.y += 1
        if not self.compact:
            self.y += 1

    def worked(self, label: str = "Worked for 1.8s"):
        if not self.ok():
            return
        self.s.put(self.tx, self.y, label, S_DIM)
        self.y += 1
        if not self.compact:
            self.y += 1

    def line(self, parts, indent: int = 0):
        if not self.ok():
            return
        self.s.spans(self.tx + indent, self.y, parts, max_x=self.x1 - 1)
        self.y += 1

    def dim(self, text: str, indent: int = 0):
        self.line([(clip(text, self.x1 - 1 - self.tx - indent), S_DIM)], indent)

    def tile(self, tool: str, arg: str, meta: str = "", glyph: str = "●", glyph_st: St = S_DIM, meta_parts=None):
        if not self.ok():
            return
        x = self.s.spans(self.tx, self.y, [(glyph, glyph_st), (" ", S), (tool, S_BOLD), ("  ", S)])
        room = self.x1 - 1 - x - (len(meta) + 3 if meta else 0)
        x = self.s.put(x, self.y, clip(arg, room), S)
        if meta_parts:
            self.s.spans(x, self.y, [(" · ", S_DIM)] + meta_parts, max_x=self.x1 - 1)
        elif meta:
            self.s.spans(x, self.y, [(" · ", S_DIM), (meta, S_DIM)], max_x=self.x1 - 1)
        self.y += 1

    def command(self, cmd: str):
        """A command row on the user-turn gray (approval prompts, sudo)."""
        if not self.ok():
            return
        self.s.fill(self.x0, self.y, self.x1, BAR_USER)
        self.s.put(self.tx, self.y, clip(cmd, self.x1 - 1 - self.tx), St(fg=TEXT, bg=BAR_USER))
        self.y += 1

    def options(self, opts, focused: int = 0, hover: int | None = None, numbered: bool = True):
        """Inline numbered radios: focused = `#262626` bar + violet `>`; hover = `#1A1A1A` bar."""
        for i, label in enumerate(opts):
            if not self.ok():
                return
            is_focus = i == focused
            is_hover = hover is not None and hover == i and not is_focus
            bg = BAR_SEL if is_focus else (BAR_HOV if is_hover else None)
            if bg:
                self.s.fill(self.x0, self.y, self.x1, bg)
            self.s.put(self.tx, self.y, ">" if is_focus else " ", St(fg=VIOLET, bg=bg) if is_focus else S)
            x = self.tx + 2
            if numbered:
                x = self.s.put(x, self.y, f"{i + 1} ", St(fg=DIM, bg=bg) if bg else S_DIM)
            self.s.put(x, self.y, clip(label, self.x1 - 1 - x), St(fg=TEXT, bg=bg) if bg else S)
            self.y += 1


def menu(s: Screen, c: Ctx, y_bottom: int, rows, focused: int = 0, hover: int | None = None, name_w: int = 18, trailer: str | None = None) -> int:
    """Rows stacked directly above ``y_bottom`` (the composer top). Returns top row.

    rows: list of (name_parts, desc) — name_parts is a span list."""
    n = len(rows) + (1 if trailer else 0)
    top = y_bottom - n
    y = top
    longest = max((sum(len(t) for t, _ in parts) for parts, _ in rows), default=0)
    name_w = max(name_w, longest + 2) if any(desc for _, desc in rows) else 0
    for idx, (name_parts, desc) in enumerate(rows):
        is_focus = idx == focused
        is_hover = hover is not None and idx == hover and not is_focus
        bg = BAR_SEL if is_focus else (BAR_HOV if is_hover else None)
        if bg:
            s.fill(c.x0, y, c.x1, bg)
        s.put(c.x0 + 2, y, ">" if is_focus else " ", St(fg=VIOLET, bg=bg) if is_focus else S)
        x = c.x0 + 4
        for text, st in name_parts:
            x = s.put(x, y, text, st(bg=bg) if bg else st, max_x=c.x1 - 1)
        dx = c.x0 + 4 + name_w
        s.put(dx, y, clip(desc, c.x1 - 1 - dx), St(fg=DIM, bg=bg) if bg else S_DIM)
        y += 1
    if trailer:
        s.put(c.x0 + 4, y, clip(trailer, c.x1 - 1 - c.x0 - 4), S_MUTED)
    return top


def modal(s: Screen, c: Ctx, title: str, w: int | None = None, h: int | None = None, close: bool = True):
    """Centered rounded modal with the title in the top hairline. Returns (x, y, w, h)."""
    if c.narrow:
        x, y, w, h = 0, 0, c.cols, c.rows
    else:
        w = w or min(96, c.cols - 4)
        h = h or (c.rows - 2)
        x = (c.cols - w) // 2
        y = 1
    s.clear_rect(x, y, x + w, y + h)
    s.box(x, y, w, h, fg=HAIR)
    s.put(x + 2, y, f" {title} ", S_BOLD)
    if close:
        s.right(y, " [x] ", x + w - 2, S_DIM)
    return x, y, w, h


def legend(s: Screen, y: int, x0: int, x1: int, items, sep: str = " | "):
    """Centered key legend: key in text colour, label dim."""
    parts = []
    for k, (key, label) in enumerate(items):
        if k:
            parts.append((sep, S_MUTED))
        parts.append((key, St(fg=TEXT)))
        parts.append((" " + label, S_DIM))
    s.center_spans(y, parts, x0, x1)


def search_field(s: Screen, x: int, y: int, w: int, typed: str = "", placeholder: str = "/ to search", focused: bool = False):
    if typed:
        s.spans(x, y, [("/ ", S_ACC if focused else S_DIM), (typed, S)])
        if focused:
            s.caret = (x + 2 + len(typed), y)
    else:
        s.put(x, y, placeholder, S_DIM)
    s.hline(y + 1, x, x + w, HAIR)


# --------------------------------------------------------------------------- #
# Content
# --------------------------------------------------------------------------- #

REPLY_ABOUT = [
    "I'm **Cortex**, a coding agent that runs in your terminal.",
    "",
    "I mostly help you **build and debug software**: code, architecture, debugging, reviews, docs, and a bit of research. "
    "Here I run in an interactive terminal, so I can read your files, run commands, and change the project.",
    "",
    "In practice:",
    "• I get straight to the point",
    "• I prefer concrete work over long explanations",
    "• I can also discuss, explain, or help you plan",
    "",
    "Tell me what you'd like to do.",
]

PALETTE_HOME = [
    ("/model", "Choose the model for this session"),
    ("/mode", "Switch between Agent, Plan and Ask"),
    ("/permissions", "Set the approval policy for edits and commands"),
    ("/plan", "Draft a plan before writing any code"),
    ("/effort", "Tune reasoning effort for the current model"),
    ("/mcp", "View and manage MCP servers"),
    ("/sandbox", "Configure sandboxed command execution"),
    ("/usage", "Plan usage, quota and limits"),
    ("/resume", "Resume a previous session"),
    ("/settings", "Open settings panel"),
]

PALETTE_MOD = [
    ("/model", [0, 1, 2, 3], "Choose the model for this session"),
    ("/mode", [0, 1, 2, 3], "Switch between Agent, Plan and Ask"),
    ("/mcp-reload", [0, 1, 8, 10], "Reconnect every configured MCP server"),
    ("/custom-commands", [0, 6, 9, 14], "Manage custom commands"),
]

MODELS = [
    ("Cortex Mini 1", "Fast default for everyday coding", "current"),
    ("Cortex 1", "Deeper reasoning for hard changes", ""),
    ("Cortex Max 1", "Longest context — bills by token instead of per request", "MAX"),
]

EFFORTS = [
    ("High Effort", "Deepest reasoning — best for hard, multi-file changes"),
    ("Medium Effort", "Balanced reasoning for everyday coding · default"),
    ("Low Effort", "Fastest responses for quick edits and questions"),
]

SETTINGS = [
    (
        "Appearance",
        [
            ("Compact mode", "off", False),
            ("Default screen mode", "Fullscreen", True),
            ("Show timestamps", "on", False),
            ("Show thinking blocks", "on", False),
            ("Group tool calls", "on", False),
            ("Collapsed edit blocks", "off", False),
            ("Line numbers", "on", False),
            ("Word wrap", "on", False),
            ("Syntax highlight", "on", False),
            ("Animations", "on", False),
            ("Theme", "Cortex Night", True),
        ],
    ),
    (
        "Mouse",
        [
            ("Mouse capture", "on", False),
            ("Scroll lines", "3", False),
            ("Invert scroll", "off", False),
            ("Copy on select", "off", False),
        ],
    ),
    (
        "Behavior",
        [
            ("Auto approve", "off", False),
            ("Sandbox mode", "on", False),
            ("Streaming", "on", False),
            ("Auto scroll", "on", False),
            ("Sound", "off", False),
            ("Notifications", "off", False),
        ],
    ),
    ("AI", [("Thinking mode", "on", False), ("Context aware", "on", False), ("Debug mode", "off", False)]),
    ("Git", [("Co-author", "on", False), ("Auto commit", "off", False), ("Sign commits", "off", False)]),
    ("Cloud", [("Cloud sync", "off", False), ("Auto save", "on", False), ("Session history", "on", False)]),
    ("Privacy", [("Telemetry", "off", False), ("Analytics", "off", False)]),
]

THEMES = [
    ("Cortex Night", "Default inky chrome · violet on focus only", True),
    ("Cortex Day", "Light chrome for bright rooms", False),
    ("Ocean Dark", "Deep blue and cyan accents", False),
    ("Monokai", "Classic code-editor colors", False),
]


# --------------------------------------------------------------------------- #
# Shared scenes
# --------------------------------------------------------------------------- #


def scene_about(s: Screen, c: Ctx, limit: int, compact: bool = False, first_turn: bool = True) -> Flow:
    f = Flow(s, c, y=1 if compact else 2, limit=limit, compact=compact)
    if first_turn:
        f.user("hey", "12:49 AM")
        f.thought("Thought for 0.4s")
        f.reply(["Hey — what do you want to work on?"], "12:49 AM")
        f.worked("Worked for 1.8s")
    f.user("tell me about yourself", "12:49 AM")
    f.thought("Thought for 0.4s")
    f.reply(REPLY_ABOUT, "12:49 AM")
    f.worked("Worked for 4.6s")
    return f


def scene_shell(s: Screen, c: Ctx, limit: int) -> Flow:
    f = Flow(s, c, limit=limit)
    f.user("run the tui tests and fix whatever fails", "09:14 AM")
    f.thought("Thought for 2.1s")
    f.tile("Shell", "$ cargo test -p cortex-tui", meta_parts=[("✓ 0", S_OK), (" · 41s", S_DIM)])
    f.tile("Read", "src/cortex-tui/src/composer.rs", "212 lines")
    f.tile("Grep", '"alternate_screen" in src/', "6 hits in 4 files")
    f.blank()
    return f


def backdrop_tail(s: Screen, c: Ctx, y_end: int):
    """Paint the *tail* of the about-conversation so its last row sits at
    ``y_end - 2`` — a picker pushes the transcript up, so the newest rows stay
    visible and the oldest scroll off under the header."""
    scratch = Screen(c.cols, 80)
    scene_about(scratch, Ctx(c.cols, 80, c.narrow), limit=78)
    last = max(y for y in range(80) if any(cell.ch != " " for cell in scratch.cells[y]))
    shift = (y_end - 2) - last
    for y in range(1, y_end - 1):
        src = y - shift
        if 2 <= src < 80:
            s.cells[y] = [Cell(ch=cl.ch, fg=cl.fg, bg=cl.bg, b=cl.b, i=cl.i, u=cl.u) for cl in scratch.cells[src]]


# --------------------------------------------------------------------------- #
# A. Entry / welcome
# --------------------------------------------------------------------------- #


def welcome_lines(s: Screen, c: Ctx, title_parts, y: int = 2):
    s.spans(c.x0 + 2, y, title_parts, max_x=c.x1)
    hints = [("v" + VERSION, S_DIM), ("  ·  ", S_MUTED), ("/", S), (" commands", S_DIM), ("  ·  ", S_MUTED), ("@", S), (" files", S_DIM)]
    if not c.narrow:
        hints += [("  ·  ", S_MUTED), ("!", S), (" shell", S_DIM), ("  ·  ", S_MUTED), ("&", S), (" cloud", S_DIM)]
    s.spans(c.x0 + 2, y + 1, hints, max_x=c.x1)


def board_welcome_cortex(s, c):
    header(s, c, "0 / 500K")
    title = [("Welcome to ", S), ("Cortex", S_BOLD)] + ([] if c.narrow else [(", the coding agent CLI", S_DIM)])
    welcome_lines(s, c, title)
    composer(s, c)
    footer(s, c)


def board_welcome_agent(s, c):
    header(s, c, "0 / 500K")
    title = [("Welcome to ", S), ("Cortex Agent", S_BOLD)] + ([] if c.narrow else [(" — describe a task, it does the work", S_DIM)])
    welcome_lines(s, c, title)
    composer(s, c, placeholder="Describe a task for the agent")
    footer(s, c)


def board_first_run(s, c):
    header(s, c, "0 / 500K")
    title = [("Welcome to ", S), ("Cortex", S_BOLD)] + ([] if c.narrow else [(", the coding agent CLI", S_DIM)])
    welcome_lines(s, c, title)
    # charcoal tips panel
    y = 5
    if c.narrow:
        s.fill_rect(c.x0, y, c.x1, y + 3, PANEL)
        s.put(c.x0 + 1, y, "A few tips to get started:", St(fg=TEXT, bg=PANEL, b=True))
        s.spans(c.x0 + 1, y + 1, [("1. ", St(fg=DIM, bg=PANEL)), ("/model", St(fg=TEXT, bg=PANEL)), (" switches model + effort.", St(fg=DIM, bg=PANEL))], max_x=c.x1 - 1)
        s.spans(c.x0 + 1, y + 2, [("2. ", St(fg=DIM, bg=PANEL)), ("@", St(fg=TEXT, bg=PANEL)), (" files add context.", St(fg=DIM, bg=PANEL))], max_x=c.x1 - 1)
    else:
        s.fill_rect(c.x0, y, c.x1, y + 5, PANEL)
        p = St(fg=DIM, bg=PANEL)
        t = St(fg=TEXT, bg=PANEL)
        s.put(c.x0 + 2, y, "A few tips to get the most out of this tool:", St(fg=TEXT, bg=PANEL, b=True))
        s.spans(c.x0 + 2, y + 1, [("1. Use ", p), ("/model", t), (" to switch between models and adjust reasoning effort.", p)])
        s.spans(c.x0 + 2, y + 2, [("2. Add ", p), ("@", t), (" files to give Cortex the right context.", p)])
        s.spans(c.x0 + 2, y + 3, [("3. Press ", p), ("Shift+Tab", t), (" anytime to cycle Agent / Plan / Ask.", p)])
        s.spans(c.x0 + 2, y + 4, [("4. ", p), ("Ctrl+x", t), (" lists every shortcut · ", p), ("F2", t), (" opens settings.", p)])
    composer(s, c)
    footer(s, c)


def board_session_empty(s, c):
    header(s, c)
    composer(s, c)
    footer(s, c)


# --------------------------------------------------------------------------- #
# B. Session
# --------------------------------------------------------------------------- #


def board_session_user_bars(s, c):
    header(s, c)
    top = composer(s, c)
    if c.narrow:
        f = Flow(s, c, limit=top)
        f.user("hey", "12:49 AM")
        f.thought("Thought for 0.4s")
        f.reply(["Hey — what do you want to work on?"])
        f.worked("Worked for 1.8s")
        footer(s, c)
        return
    banner_rows = 4
    banner_y = top - 1 - banner_rows
    scene_about(s, c, limit=banner_y - 2)
    scroll_indicator(s, c, banner_y - 2)
    optin_banner(s, c, banner_y)
    footer(s, c)


def board_session_thought(s, c):
    header(s, c)
    top = composer(s, c)
    f = Flow(s, c, limit=top - 1)
    f.user("why does the composer lose focus after /model?", "10:02 AM")
    f.thought("Thought for 3.2s")
    f.reply(["The picker steals focus and never hands it back. `close_picker()` returns early when the effort radios are open."], "10:02 AM")
    f.worked("Worked for 6.0s")
    footer(s, c)


def board_session_thought_expanded(s, c):
    header(s, c)
    top = composer(s, c)
    f = Flow(s, c, limit=top - 1)
    f.user("why does the composer lose focus after /model?", "10:02 AM")
    f.thought("Thought for 3.2s ▾")
    f.thought_body(
        [
            "The user reports focus loss after the model picker closes.",
            "Reading composer.rs — close_picker() early-returns when effort is open,",
            "so focus_composer() is skipped. That matches the report.",
        ]
    )
    f.reply(["The picker steals focus and never hands it back. `close_picker()` returns early when the effort radios are open."], "10:02 AM")
    footer(s, c)


def board_session_thinking_live(s, c):
    header(s, c)
    top = composer(s, c, placeholder="Add a follow-up — Enter to queue", focused=False, caret=False)
    f = Flow(s, c, limit=top - 1)
    f.user("why does the composer lose focus after /model?", "10:02 AM")
    f.thought("Thinking · 3s", live=True)
    footer(s, c, [("Esc", "interrupt"), ("Enter", "queue follow-up"), ("Ctrl+x", "shortcuts")] if not c.narrow else [("Esc", "interrupt"), ("Ctrl+x", "shortcuts")])


def board_session_assistant(s, c):
    header(s, c)
    top = composer(s, c)
    f = Flow(s, c, limit=top - 1)
    f.user("tell me about yourself", "12:49 AM")
    f.thought("Thought for 0.4s")
    f.reply(REPLY_ABOUT if not c.narrow else ["I'm **Cortex**, a coding agent in your terminal.", "• straight to the point", "• concrete work first"], "12:49 AM")
    footer(s, c)


def board_session_worked(s, c):
    header(s, c)
    top = composer(s, c)
    f = Flow(s, c, limit=top - 1)
    f.user("hey", "12:49 AM")
    f.thought("Thought for 0.4s")
    f.reply(["Hey — what do you want to work on?"], "12:49 AM")
    f.worked("Worked for 1.8s")
    footer(s, c)


def board_session_optin(s, c):
    header(s, c)
    top = composer(s, c)
    rows = 3 if c.narrow else 4
    banner_y = top - 1 - rows
    f = Flow(s, c, limit=banner_y - 1)
    f.user("hey", "12:49 AM")
    f.thought("Thought for 0.4s")
    f.reply(["Hey — what do you want to work on?"], "12:49 AM")
    f.worked("Worked for 1.8s")
    optin_banner(s, c, banner_y)
    footer(s, c)


def board_session_optin_hover(s, c):
    header(s, c)
    top = composer(s, c)
    rows = 3 if c.narrow else 4
    banner_y = top - 1 - rows
    f = Flow(s, c, limit=banner_y - 1)
    f.user("hey", "12:49 AM")
    f.thought("Thought for 0.4s")
    f.reply(["Hey — what do you want to work on?"], "12:49 AM")
    f.worked("Worked for 1.8s")
    optin_banner(s, c, banner_y, hover="in")
    footer(s, c)


def board_composer_empty(s, c):
    header(s, c)
    composer(s, c)
    footer(s, c)


def board_composer_typing(s, c):
    header(s, c)
    composer(s, c, content=[("add retry with backoff to the api client", S)])
    footer(s, c, FOOTER_TYPED_NARROW if c.narrow else FOOTER_TYPED)


def board_composer_typing_blink(s, c):
    header(s, c)
    composer(s, c, content=[("add retry with backoff to the api client", S)], caret=False)
    footer(s, c, FOOTER_TYPED_NARROW if c.narrow else FOOTER_TYPED)


def board_composer_hover(s, c):
    header(s, c)
    composer(s, c, hover=True)
    footer(s, c)


def board_composer_multiline(s, c):
    header(s, c)
    bottom = c.rows - 3
    top = bottom - 4
    s.box(c.x0, top, c.inner_w, 5)
    s.put(c.x0 + 2, top, " Agent ", S_DIM)
    s.right(bottom, f" {MODEL_CHIP} ", c.x1 - 2, S_DIM)
    s.put(c.x0 + 2, top + 1, ">", S_ACC)
    lines = ["add retry with backoff to the api client:", "- max 3 attempts, jitter", "- surface the final error as product copy"]
    for i, ln in enumerate(lines):
        s.put(c.x0 + 4, top + 1 + i, clip(ln, c.inner_w - 6), S)
    s.caret = (c.x0 + 4 + len(lines[-1]), top + 3)
    footer(s, c, FOOTER_TYPED_NARROW if c.narrow else FOOTER_TYPED)


def board_footer_shortcuts(s, c):
    header(s, c)
    composer(s, c, content=[("hello", S)])
    footer(s, c, FOOTER_TYPED_NARROW if c.narrow else FOOTER_TYPED)


def board_footer_hover(s, c):
    header(s, c)
    composer(s, c)
    footer(s, c, FOOTER_IDLE, hover="Ctrl+x")


def board_tokens_topright(s, c):
    header(s, c, "142K / 500K")
    top = composer(s, c)
    scene_shell(s, c, limit=top - 1)
    footer(s, c)


def board_tokens_topright_warn(s, c):
    header(s, c, "462K / 500K", warn=True)
    top = composer(s, c)
    f = scene_shell(s, c, limit=top - 1)
    f.line([("Context is 92% full — ", S_WARN), ("/compact", S), (" summarizes the thread to free room.", S_DIM)])
    footer(s, c)


def board_compact_chat(s, c):
    header(s, c)
    top = composer(s, c)
    scene_about(s, c, limit=top, compact=True)
    footer(s, c)


# --------------------------------------------------------------------------- #
# C. Slash + model
# --------------------------------------------------------------------------- #


def slash_rows(entries, typed: str = ""):
    rows = []
    for name, desc in entries:
        parts = []
        if typed and name.startswith(typed):
            parts.append((typed, S_ACC))
            parts.append((name[len(typed) :], S))
        else:
            parts.append((name, S))
        rows.append((parts, desc))
    return rows


def board_slash_palette(s, c):
    header(s, c)
    top = composer(s, c, content=[("/", S_ACC)])
    n = 3 if c.narrow else 8
    rows = slash_rows(PALETTE_HOME[:n], "/")
    trailer = None if c.narrow else "… 87 more — keep typing to filter"
    menu_top = menu(s, c, top, rows, focused=0, hover=3 if not c.narrow else None, name_w=16, trailer=trailer)
    backdrop_tail(s, c, menu_top)
    footer(s, c, FOOTER_TYPED_NARROW if c.narrow else FOOTER_TYPED)


def fuzzy_parts(name: str, idxs) -> list:
    parts = []
    for i, ch in enumerate(name):
        parts.append((ch, S_ACC if i in idxs else S))
    return parts


def board_slash_model_typed(s, c):
    header(s, c)
    top = composer(s, c, content=[("/mod", S_ACC), ("el", S_DIM)], caret=True)
    # caret should sit after the typed part, before the ghost completion
    s.caret = (c.x0 + 4 + 4, top + 1)
    rows = [(fuzzy_parts(n, i), d) for n, i, d in PALETTE_MOD]
    if c.narrow:
        rows = rows[:3]
    menu_top = menu(s, c, top, rows, focused=0, name_w=18)
    backdrop_tail(s, c, menu_top)
    footer(s, c, FOOTER_TYPED_NARROW if c.narrow else FOOTER_TYPED)


def model_rows():
    rows = []
    for name, desc, meta in MODELS:
        d = desc + (f" · {meta}" if meta else "")
        rows.append(([(name, S)], d))
    return rows


def board_model_list(s, c):
    header(s, c)
    top = composer(s, c, content=[("/model", S_ACC), (" ", S)])
    menu_top = menu(s, c, top, model_rows(), focused=0, name_w=16)
    backdrop_tail(s, c, menu_top)
    footer(s, c, [("Enter", "choose"), ("Tab", "effort"), ("Esc", "close")] if c.narrow else [("Enter", "choose"), ("Tab", "effort"), ("↑↓", "select"), ("Esc", "close")])


def board_model_list_hover(s, c):
    header(s, c)
    top = composer(s, c, content=[("/model", S_ACC), (" ", S)])
    menu_top = menu(s, c, top, model_rows(), focused=0, hover=2, name_w=16)
    backdrop_tail(s, c, menu_top)
    footer(s, c, [("Enter", "choose"), ("Tab", "effort"), ("Esc", "close")] if c.narrow else [("Enter", "choose"), ("Tab", "effort"), ("↑↓", "select"), ("Esc", "close")])


def effort_board(s, c, focused: int, hover: int | None = None):
    header(s, c)
    top = composer(s, c, content=[("/model", S_ACC), (" Cortex Mini 1", S)])
    rows = [([(n, S)], d) for n, d in EFFORTS]
    menu_top = menu(s, c, top, rows, focused=focused, hover=hover, name_w=15)
    backdrop_tail(s, c, menu_top)
    footer(s, c, [("Enter", "apply"), ("Tab", "back to models"), ("Esc", "close")] if not c.narrow else [("Enter", "apply"), ("Esc", "close")])


def board_model_effort_high(s, c):
    effort_board(s, c, 0)


def board_model_effort_medium(s, c):
    effort_board(s, c, 1)


def board_model_effort_low(s, c):
    effort_board(s, c, 2)


def board_model_effort_hover(s, c):
    effort_board(s, c, 1, hover=2)


# --------------------------------------------------------------------------- #
# D. Settings
# --------------------------------------------------------------------------- #


def settings_lines(filter_text: str = ""):
    """Flatten SETTINGS into ('cat', name) / ('row', label, value, sub) / ('gap',)."""
    lines = []
    for k, (cat, rows) in enumerate(SETTINGS):
        vis = [r for r in rows if not filter_text or filter_text.lower() in r[0].lower()]
        if not vis:
            continue
        if lines:
            lines.append(("gap",))
        lines.append(("cat", cat))
        for label, value, sub in vis:
            lines.append(("row", label, value, sub))
    return lines


def paint_settings(s: Screen, c: Ctx, focus_label: str, hover_label: str | None = None, scroll: int = 0, search: str = "", search_focused: bool = False):
    x, y, w, h = modal(s, c, "Settings")
    ix0, ix1 = x + 2, x + w - 2  # inner content columns
    search_field(s, ix0, y + 1, ix1 - ix0, typed=search, focused=search_focused)
    body_top = y + 3 if c.narrow else y + 4
    if c.narrow:
        body_bottom = y + h - 3
    else:
        body_bottom = y + h - 5
    lines = settings_lines(search)[scroll:]
    row_y = body_top
    for ln in lines:
        if row_y >= body_bottom:
            break
        if ln[0] == "gap":
            row_y += 1
            continue
        if ln[0] == "cat":
            s.put(ix0, row_y, ln[1], S_DIM)
            s.hline(row_y, ix0 + len(ln[1]) + 1, ix1, HAIR)
            row_y += 1
            continue
        _, label, value, sub = ln
        is_focus = label == focus_label
        is_hover = hover_label == label and not is_focus
        bg = BAR_SEL if is_focus else (BAR_HOV if is_hover else None)
        if bg:
            s.fill(x + 1, row_y, x + w - 1, bg)
        marker_st = St(fg=VIOLET, bg=bg) if is_focus else St(fg=DIM, bg=bg) if bg else S_DIM
        s.put(ix0 + 1, row_y, "▸", marker_st)
        lab_st = St(fg=TEXT, b=is_focus, bg=bg) if bg else S
        val_st = St(fg=TEXT if value != "off" else DIM, bg=bg) if bg else (S if value != "off" else S_DIM)
        if c.narrow and sub:
            value = "" if len(label) + len(value) + 10 > (ix1 - ix0) else value
        val_len = len(value) + (3 if sub else 0)
        label = clip(label, ix1 - 1 - val_len - 1 - (ix0 + 3))
        if search and search.lower() in label.lower():
            k = label.lower().find(search.lower())
            pre, mid, post = label[:k], label[k : k + len(search)], label[k + len(search) :]
            s.spans(ix0 + 3, row_y, [(pre, lab_st), (mid, St(fg=VIOLET, b=is_focus, bg=bg)), (post, lab_st)])
        else:
            s.put(ix0 + 3, row_y, label, lab_st)
        if sub:
            s.right_spans(row_y, [(value, val_st), ("  >", St(fg=DIM, bg=bg) if bg else S_DIM)], ix1 - 1)
        else:
            s.right(row_y, value, ix1 - 1, val_st)
        row_y += 1
    # footer
    if c.narrow:
        legend(s, y + h - 3, x + 1, x + w - 1, [("↑↓", "nav"), ("Space", "toggle"), ("/", "search")])
        legend(s, y + h - 2, x + 1, x + w - 1, [("F2/Esc", "close")])
    else:
        s.center_spans(
            y + h - 4,
            [("Tip · Ask Cortex: ", S_MUTED), ('"change theme to Cortex Day"', S_DIM), (" or ", S_MUTED), ('"what does compact mode do?"', S_DIM)],
            x + 1,
            x + w - 1,
        )
        legend(
            s,
            y + h - 3,
            x + 1,
            x + w - 1,
            [("↑/↓/j/k", "nav"), ("g/G", "top/btm"), ("Space", "toggle"), ("Enter", "toggle"), ("→", "expand"), ("/", "search"), ("d", "reset")],
        )
        legend(s, y + h - 2, x + 1, x + w - 1, [("F2/Esc", "close")])
    return x, y, w, h


def settings_backdrop(s, c):
    header(s, c)
    top = composer(s, c, focused=False, caret=False)
    scene_about(s, c, limit=top - 1)
    footer(s, c)


def board_settings_appearance(s, c):
    settings_backdrop(s, c)
    paint_settings(s, c, focus_label="Compact mode")


def board_settings_mouse(s, c):
    settings_backdrop(s, c)
    # scrolled so the Mouse category leads the list
    paint_settings(s, c, focus_label="Scroll lines", scroll=13)


def board_settings_row_hover(s, c):
    settings_backdrop(s, c)
    paint_settings(s, c, focus_label="Compact mode", hover_label="Show timestamps")


def board_settings_search(s, c):
    settings_backdrop(s, c)
    paint_settings(s, c, focus_label="Scroll lines", search="scro", search_focused=True)


def board_settings_theme_submenu(s, c):
    settings_backdrop(s, c)
    x, y, w, h = modal(s, c, "Settings")
    ix0, ix1 = x + 2, x + w - 2
    s.spans(ix0, y + 1, [("Appearance", S_DIM), (" › ", S_MUTED), ("Theme", S)])
    s.hline(y + 2, ix0, ix1, HAIR)
    row_y = y + 3 if c.narrow else y + 4
    for k, (name, desc, current) in enumerate(THEMES):
        is_focus = k == 0
        bg = BAR_SEL if is_focus else None
        if bg:
            s.fill(x + 1, row_y, x + w - 1, bg)
        s.put(ix0 + 1, row_y, "●" if current else "○", St(fg=TEXT if current else DIM, bg=bg) if bg else (S if current else S_DIM))
        s.put(ix0 + 3, row_y, name, St(fg=VIOLET if is_focus else TEXT, b=is_focus, bg=bg) if bg else S)
        if not c.narrow:
            s.put(ix0 + 18, row_y, desc, St(fg=DIM, bg=bg) if bg else S_DIM)
        if current:
            s.right(row_y, "current", ix1 - 1, St(fg=DIM, bg=bg) if bg else S_DIM)
        row_y += 1
    if c.narrow:
        legend(s, y + h - 3, x + 1, x + w - 1, [("Enter", "select"), ("←", "back")])
        legend(s, y + h - 2, x + 1, x + w - 1, [("F2/Esc", "close")])
    else:
        s.center_spans(y + h - 4, [("Themes never change the accent rule: violet on focus only.", S_MUTED)], x + 1, x + w - 1)
        legend(s, y + h - 3, x + 1, x + w - 1, [("↑/↓/j/k", "nav"), ("Enter", "select"), ("←", "back"), ("d", "reset")])
        legend(s, y + h - 2, x + 1, x + w - 1, [("F2/Esc", "close")])


# --------------------------------------------------------------------------- #
# E. Modes / tools / errors
# --------------------------------------------------------------------------- #


def board_mode_agent(s, c):
    header(s, c)
    top = composer(s, c, mode="Agent")
    scene_shell(s, c, limit=top - 1)
    footer(s, c)


def board_mode_plan(s, c):
    header(s, c)
    top = composer(s, c, mode="Plan", placeholder="Describe what you want — Cortex drafts a plan first")
    f = Flow(s, c, limit=top - 1)
    f.user("add retry with backoff to the api client", "11:20 AM")
    f.thought("Thought for 5.8s")
    f.reply(
        [
            "**Plan** — no files change until you approve.",
            "",
            "1. Wrap `ApiClient::send` in a retry loop: 3 attempts, exponential backoff with jitter.",
            "2. Retry only on timeouts and 5xx; never on 4xx.",
            "3. Surface the final failure as product copy: 'The coding service is temporarily unavailable'.",
            "4. Unit tests for the backoff schedule and the give-up path.",
        ]
        if not c.narrow
        else ["**Plan** — no edits until approved.", "1. retry loop, 3 attempts", "2. tests for backoff"],
        "11:20 AM",
    )
    footer(s, c)


def board_mode_ask(s, c):
    header(s, c)
    top = composer(s, c, mode="Ask", placeholder="Ask about the codebase — read-only")
    f = Flow(s, c, limit=top - 1)
    f.user("where is the alternate-screen default decided?", "11:31 AM")
    f.thought("Thought for 1.1s")
    f.reply(
        ["`TuiConfig::default()` in `src/cortex-engine/src/config/types.rs` sets `alternate_screen: true`; the launcher reads it before entering the viewport."]
        if not c.narrow
        else ["`TuiConfig::default()` sets `alternate_screen: true`."],
        "11:31 AM",
    )
    footer(s, c)


def board_mode_bash(s, c):
    header(s, c)
    composer(s, c, mode="Bash", content=[("cargo test -p cortex-tui", S)], sigil="!")
    footer(s, c, [("Enter", "run"), ("Esc", "leave bash"), ("Ctrl+x", "shortcuts")] if not c.narrow else [("Enter", "run"), ("Esc", "leave bash")])


def board_permission_prompt(s, c):
    header(s, c)
    top = composer(s, c, focused=False, caret=False, placeholder="Choose an option above")
    f = Flow(s, c, y=1 if c.narrow else 2, limit=top if c.narrow else top - 1)
    if not c.narrow:
        f.user("add ioredis and a mock for the tests", "09:40 AM")
        f.thought("Thought for 1.4s")
    f.line([("●", S_DIM), (" Cortex wants to run", S)])
    f.command("$ npm install ioredis && npm install -D ioredis-mock")
    f.options(["Yes, run once", "Yes, always allow npm install in this project", "Edit command", "No — tell Cortex what to do instead"], focused=0)
    footer(s, c, [("↑↓", "select"), ("Enter", "confirm"), ("e", "edit command"), ("Esc", "cancel")] if not c.narrow else [("Enter", "confirm"), ("Esc", "cancel")])


def board_permission_prompt_hover(s, c):
    header(s, c)
    top = composer(s, c, focused=False, caret=False, placeholder="Choose an option above")
    f = Flow(s, c, limit=top - 1)
    f.user("add ioredis and a mock for the tests", "09:40 AM")
    f.thought("Thought for 1.4s")
    f.line([("●", S_DIM), (" Cortex wants to run", S)])
    f.command("$ npm install ioredis && npm install -D ioredis-mock")
    f.options(["Yes, run once", "Yes, always allow npm install in this project", "Edit command", "No — tell Cortex what to do instead"], focused=0, hover=1)
    footer(s, c, [("↑↓", "select"), ("Enter", "confirm"), ("e", "edit command"), ("Esc", "cancel")])


def board_permissions_picker(s, c):
    header(s, c)
    top = composer(s, c, content=[("/permissions", S_ACC), (" ", S)])
    rows = [
        ([("● ", S), ("Smart", S)], "auto-approve safe reads · ask before edits · current"),
        ([("○ ", S_DIM), ("Read-only", S)], "never edit files or run commands"),
        ([("○ ", S_DIM), ("Full access", S)], "only ask when leaving the sandbox"),
    ]
    menu_top = menu(s, c, top, rows, focused=0, name_w=15)
    s.spans(c.x0 + 2, menu_top - 2, [("Permissions", S), (" · how Cortex asks before acting", S_DIM)], max_x=c.x1)
    backdrop_tail(s, c, menu_top - 2)
    footer(s, c, [("Enter", "apply"), ("Esc", "close")])


def board_mcp_servers(s, c):
    header(s, c)
    top = composer(s, c, content=[("/mcp", S_ACC), (" ", S)])
    rows = [
        ([("✓ ", S_OK), ("github", S)], "12 tools · connected"),
        ([("✓ ", S_OK), ("filesystem", S)], "8 tools · connected"),
        ([("⠇ ", S_DIM), ("linear", S)], "authenticating…"),
        ([("× ", S_ERR), ("sentry", S)], "failed — token expired · r to reconnect"),
    ]
    if c.narrow:
        rows = rows[:3]
    menu_top = menu(s, c, top, rows, focused=0, name_w=15)
    s.spans(c.x0 + 2, menu_top - 2, [("MCP servers", S), (" · 2 of 4 connected", S_DIM)], max_x=c.x1)
    if not c.narrow:
        backdrop_tail(s, c, menu_top - 2)
    footer(s, c, [("Enter", "details"), ("r", "reconnect"), ("a", "add server"), ("Esc", "close")] if not c.narrow else [("Enter", "details"), ("Esc", "close")])


def board_mcp_drop(s, c):
    header(s, c)
    top = composer(s, c)
    f = scene_shell(s, c, limit=top - 1)
    f.line([("×", S_ERR), (" github", S), (" dropped", S_DIM)])
    f.dim("Reconnecting 2/3 — tools from github are paused until it is back.", indent=2)
    footer(s, c)


def board_plugins(s, c):
    header(s, c)
    top = composer(s, c, content=[("/plugins", S_ACC), (" ", S)])
    rows = [
        ([("✓ ", S_OK), ("cortex-review", S)], "1.2.0 · enabled · adds /review"),
        ([("✓ ", S_OK), ("mermaid-preview", S)], "0.4.1 · enabled"),
        ([("○ ", S_DIM), ("jira", S)], "0.9.0 · disabled"),
    ]
    menu_top = menu(s, c, top, rows, focused=0, name_w=20)
    s.spans(c.x0 + 2, menu_top - 2, [("Plugins", S), (" · 2 of 3 enabled", S_DIM)], max_x=c.x1)
    backdrop_tail(s, c, menu_top - 2)
    footer(s, c, [("Enter", "toggle"), ("i", "install"), ("u", "update"), ("Esc", "close")] if not c.narrow else [("Enter", "toggle"), ("Esc", "close")])


def bar_cells(used: int, total: int, width: int, fg_full: str = DIM) -> list:
    filled = round(width * used / total) if total else 0
    return [("█" * filled, St(fg=fg_full)), ("█" * (width - filled), St(fg=BAR_SEL))]


def board_usage(s, c):
    header(s, c)
    top = composer(s, c, content=[("/usage", S_ACC), (" ", S)])
    y = top - 7 if not c.narrow else top - 4
    s.spans(c.x0 + 2, y, [("Usage", S), (" · Cortex Pro · renews Oct 1", S_DIM)], max_x=c.x1)
    rows = [("Agent requests", 42, 500, "42 / 500"), ("Tokens this month", 84, 120, "8.4M / 12M"), ("Cloud agent minutes", 132, 400, "132 / 400")]
    for k, (label, used, total, txt) in enumerate(rows):
        yy = y + 1 + k
        pct = f"{round(100 * used / total):>3}%"
        if c.narrow:
            s.put(c.x0 + 2, yy, clip(label, 19), S)
            s.right_spans(yy, [(txt, S_DIM), ("  ", S), (pct, S)], c.x1 - 1)
        else:
            s.put(c.x0 + 2, yy, label, S)
            x = s.put(c.x0 + 24, yy, txt.rjust(10), S_DIM)
            x = s.spans(x + 3, yy, bar_cells(used, total, 40))
            s.put(x + 2, yy, pct, S_DIM)
    if not c.narrow:
        s.put(c.x0 + 2, y + 5, "MAX bills by token instead of per request — manage at cortex.foundation/billing", S_MUTED)
        backdrop_tail(s, c, y)
    footer(s, c, [("Esc", "close")])


def board_quota_exhausted(s, c):
    header(s, c)
    top = composer(s, c, placeholder="Add a follow-up — held until quota resets", focused=False, caret=False)
    f = scene_shell(s, c, limit=top - 1)
    f.line([("×", S_ERR), (" Agent quota exhausted", S_ERR)])
    yy = f.y
    if f.ok():
        x = s.put(f.tx + 2, yy, "Agent requests", S)
        x = s.put(f.tx + 20, yy, "500 / 500", S_DIM)
        s.spans(x + 3, yy, bar_cells(500, 500, 24 if not c.narrow else 6, fg_full=RED), max_x=c.x1 - 1)
        f.y += 1
    f.dim("Resets in 4h 12m. Switch to MAX token billing to continue now, or upgrade at cortex.foundation/billing", indent=2)
    footer(s, c, [("/usage", "details"), ("Ctrl+x", "shortcuts")])


def board_sandbox(s, c):
    header(s, c)
    top = composer(s, c, content=[("/sandbox", S_ACC), (" ", S)])
    rows = [
        ([("Filesystem", S)], "workspace only · ~/code/cortex"),
        ([("Network", S)], "allowlist · registry.npmjs.org, github.com"),
        ([("Escalation", S)], "ask before running outside the sandbox"),
    ]
    menu_top = menu(s, c, top, rows, focused=0, name_w=14)
    s.spans(c.x0 + 2, menu_top - 2, [("Sandbox", S), (" · ", S_DIM), ("✓ On", S_OK)], max_x=c.x1)
    backdrop_tail(s, c, menu_top - 2)
    footer(s, c, [("Space", "toggle"), ("Enter", "edit"), ("Esc", "close")])


def board_sandbox_deny(s, c):
    header(s, c)
    top = composer(s, c, focused=False, caret=False, placeholder="Choose an option above")
    f = Flow(s, c, y=1 if c.narrow else 2, limit=top if c.narrow else top - 1)
    if not c.narrow:
        f.user("install the deps with the vendor script", "02:15 PM")
        f.thought("Thought for 0.9s")
    f.tile("Shell", "$ curl -s https://example.com/install.sh | sh")
    f.line([("×", S_ERR), (" Sandbox denied", S_ERR)])
    f.dim("curl was blocked by the workspace sandbox. Network is allowlisted.", indent=2)
    f.options(["Keep blocked", "Allow once", "Allow for this session"], focused=0)
    footer(s, c, [("↑↓", "select"), ("Enter", "confirm"), ("Esc", "cancel")] if not c.narrow else [("Enter", "confirm"), ("Esc", "cancel")])


def board_cloud_handoff(s, c):
    header(s, c)
    top = composer(s, c)
    f = Flow(s, c, limit=top - 1)
    f.user("& fix the flaky login redirect test and open a PR", "03:02 PM", sigil=">")
    f.line([("↑", S), (" Handed off to Cortex Cloud", S)])
    f.line([("agent    ", S_DIM), ("ag_4f2a", S), (" · running", S_DIM)], indent=2)
    f.line([("branch   ", S_DIM), ("cortex/fix-login-redirect", S)], indent=2)
    f.line([("follow   ", S_DIM), ("cortex.foundation/agents/ag_4f2a", S), (" · or /jobs right here", S_DIM)], indent=2)
    footer(s, c)


def board_diagnostics(s, c):
    header(s, c)
    top = composer(s, c)
    f = Flow(s, c, limit=top - 1)
    f.user("wire the model chip into the composer border", "04:11 PM")
    f.thought("Thought for 2.6s")
    f.tile("Edit", "src/cortex-tui/src/composer.rs", meta_parts=[("+12", S_OK), (" ", S), ("−3", S_ERR)])
    f.tile("Diagnostics", "src/cortex-tui/src/composer.rs", "2")
    if c.narrow:
        f.line([("error ", S_ERR), ("E0308 mismatched types", S)], indent=2)
        f.line([("warn  ", S_WARN), ("unused variable `hints`", S)], indent=2)
    else:
        f.line([("error ", S_ERR), ("E0308 mismatched types — expected `Span`, found `String`", S), ("   composer.rs:42:9", S_DIM)], indent=2)
        f.line([("warn  ", S_WARN), ("unused variable `hints`", S), ("   composer.rs:57:13", S_DIM)], indent=2)
    footer(s, c)


def board_interrupt_stopped(s, c):
    header(s, c)
    top = composer(s, c, placeholder="Reply, or ↑ to edit your last message")
    f = Flow(s, c, limit=top - 1)
    f.user("run the tui tests and fix whatever fails", "09:14 AM")
    f.thought("Thought for 2.1s")
    f.tile("Shell", "$ cargo test -p cortex-tui", "stopped")
    f.line([("×", S_ERR), (" Stopped", S_ERR)])
    f.dim("Worked for 12s · 4.1k tokens")
    footer(s, c)


def board_error_unavailable(s, c):
    header(s, c)
    top = composer(s, c, placeholder="Try again — your work so far is saved")
    f = Flow(s, c, limit=top - 1)
    f.user("tell me about yourself", "12:49 AM")
    f.line([("×", S_ERR), (" The coding service is temporarily unavailable", S_ERR)])
    f.dim("Try again in a moment — your work so far is saved in this session.", indent=2)
    footer(s, c, [("Enter", "retry"), ("Ctrl+x", "shortcuts")])


def board_tool_tiles(s, c):
    header(s, c)
    top = composer(s, c)
    f = Flow(s, c, limit=top - 1)
    f.user("why do tests fail on the composer?", "09:14 AM")
    f.thought("Thought for 2.1s")
    f.line([("▾", S_DIM), (" 3 tool calls", S), ("  Read ×2 · Grep ×1", S_DIM)])
    f.tile("Read", "src/cortex-tui/src/composer.rs", "212 lines")
    f.tile("Read", "src/cortex-tui/src/tests/composer.rs", "88 lines")
    f.tile("Grep", '"model_chip" in src/', "3 hits in 2 files")
    f.tile("Shell", "$ cargo test -p cortex-tui composer", meta_parts=[("× 101", S_ERR), (" · 12s", S_DIM)])
    f.dim("test composer::chip_in_border … FAILED — expected `Cortex Mini 1 (medium)`, got ``", indent=2)
    footer(s, c)


def board_tool_tiles_collapsed(s, c):
    header(s, c)
    top = composer(s, c)
    f = Flow(s, c, limit=top - 1)
    f.user("why do tests fail on the composer?", "09:14 AM")
    f.thought("Thought for 2.1s")
    f.line([("▸", S_DIM), (" 3 tool calls", S), ("  Read ×2 · Grep ×1", S_DIM)])
    f.tile("Shell", "$ cargo test -p cortex-tui composer", meta_parts=[("× 101", S_ERR), (" · 12s", S_DIM)])
    f.blank()
    f.reply(["The chip is painted before the model name resolves, so the border shows an empty label on the first frame."], "09:15 AM")
    footer(s, c)


def board_shell_running(s, c):
    header(s, c)
    top = composer(s, c, placeholder="Add a follow-up — Enter to queue", focused=False, caret=False)
    f = Flow(s, c, limit=top - 1)
    f.user("run the tui tests and fix whatever fails", "09:14 AM")
    f.thought("Thought for 2.1s")
    f.tile("Shell", "$ cargo test -p cortex-tui", "running · 8s", glyph="⠇", glyph_st=S)
    f.dim("   Compiling cortex-tui v0.1.7 (/home/m/code/cortex/src/cortex-tui)", indent=2)
    f.dim("    Finished test [unoptimized + debuginfo] target(s) in 6.91s", indent=2)
    f.dim("     Running unittests src/lib.rs", indent=2)
    footer(s, c, [("Esc", "interrupt"), ("Enter", "queue follow-up"), ("Ctrl+x", "shortcuts")] if not c.narrow else [("Esc", "interrupt")])


def board_diff_hunk(s, c):
    header(s, c)
    top = composer(s, c)
    f = Flow(s, c, limit=top - 1)
    f.user("move the model chip into the composer border", "04:11 PM")
    f.thought("Thought for 2.6s")
    f.tile("Edit", "src/cortex-tui/src/composer.rs", meta_parts=[("+2", S_OK), (" ", S), ("−1", S_ERR)])
    g = S_MUTED
    f.line([("@@ -41,7 +41,8 @@", g)], indent=2)
    f.line([("41  41  ", g), ("    let chip = model_chip(state);", S)], indent=2)
    f.line([("42      ", g), ("-   footer.push(chip);", S_ERR)], indent=2)
    f.line([("    42  ", g), ("+   border.push_right(chip);", S_OK)], indent=2)
    f.line([("    43  ", g), ("+   footer.push(hints);", S_OK)], indent=2)
    f.line([("43  44  ", g), ("    Ok(())", S)], indent=2)
    footer(s, c)


def board_edit_collapsed(s, c):
    header(s, c)
    top = composer(s, c)
    f = Flow(s, c, limit=top - 1)
    f.user("move the model chip into the composer border", "04:11 PM")
    f.thought("Thought for 2.6s")
    f.tile("Edit", "src/cortex-tui/src/composer.rs", meta_parts=[("+2", S_OK), (" ", S), ("−1", S_ERR), (" · ▸ show diff", S_DIM)])
    f.tile("Edit", "src/cortex-tui/src/footer.rs", meta_parts=[("+4", S_OK), (" ", S), ("−9", S_ERR), (" · ▸ show diff", S_DIM)])
    f.blank()
    f.reply(["Moved the chip into the bottom hairline and dropped it from the footer."], "04:12 PM")
    footer(s, c)


def board_md_table(s, c):
    header(s, c)
    top = composer(s, c)
    f = Flow(s, c, limit=top - 1)
    f.user("compare the models", "05:00 PM")
    f.thought("Thought for 0.8s")
    if c.narrow:
        f.line([("+--------------+--------+", S_HAIR)])
        f.line([("| ", S_HAIR), ("Model        ", S_BOLD), ("| ", S_HAIR), ("Effort", S_BOLD), (" |", S_HAIR)])
        f.line([("+--------------+--------+", S_HAIR)])
        f.line([("| ", S_HAIR), ("Cortex Mini 1", S), ("| ", S_HAIR), ("Medium", S), (" |", S_HAIR)])
        f.line([("+--------------+--------+", S_HAIR)])
    else:
        rows = [("Model", "Default effort", "Context", "Billing"), ("Cortex Mini 1", "Medium", "500K", "per request"), ("Cortex 1", "High", "500K", "per request"), ("Cortex Max 1", "High", "1M", "per token")]
        widths = [15, 16, 9, 13]
        sep = "+" + "+".join("-" * (w + 2) for w in widths) + "+"
        f.line([(sep, S_HAIR)])
        for k, row in enumerate(rows):
            parts = []
            for w, cell in zip(widths, row):
                parts.append(("| ", S_HAIR))
                parts.append((cell.ljust(w + 1), S_BOLD if k == 0 else S))
            parts.append(("|", S_HAIR))
            f.line(parts)
            if k == 0:
                f.line([(sep, S_HAIR)])
        f.line([(sep, S_HAIR)])
    footer(s, c)


def board_code_fence(s, c):
    header(s, c)
    top = composer(s, c)
    f = Flow(s, c, limit=top - 1)
    f.user("show me a minimal retry helper", "05:12 PM")
    f.thought("Thought for 1.0s")
    w = f.x1 - 1 - f.tx
    f.line([("─ rust ", S_DIM), ("─" * (w - 7), S_HAIR)])
    g = S_MUTED
    code = [
        [("pub async fn ", S_BOLD), ("with_retry<F, T>(mut op: F) -> Result<T>", S)],
        [("where", S_BOLD), (" F: FnMut() -> Fut<Result<T>>,", S)],
        [("{", S)],
        [("    ", S), ("for", S_BOLD), (" attempt ", S), ("in", S_BOLD), (" 0..3 {", S)],
        [("        ", S), ("match", S_BOLD), (" op().await {", S)],
        [("            Ok(v) => ", S), ("return", S_BOLD), (" Ok(v),", S)],
        [("            Err(e) ", S), ("if", S_BOLD), (" attempt == 2 => ", S), ("return", S_BOLD), (" Err(e),", S)],
        [("            Err(_) => sleep(backoff(attempt)).await,", S)],
        [("        }", S)],
        [("    }", S)],
        [("    unreachable!()", S)],
        [("}", S)],
    ]
    if c.narrow:
        code = code[:3]
    for i, parts in enumerate(code):
        f.line([(f"{i + 1:>2}  ", g)] + parts)
    f.line([("─" * w, S_HAIR)])
    footer(s, c)


def board_login(s, c):
    # `cortex login` is an inline flow — no header, composer or footer.
    y = 1
    s.put(c.x0 + 1, y, "Welcome to Cortex CLI!", S_BOLD)
    s.put(c.x0 + 1, y + 2, "How would you like to log in?", S)
    rows = [("1 Continue with browser", "Opens cortex.foundation/cli/auth"), ("2 Paste an API key", "Enter your key to authenticate")]
    for k, (label, desc) in enumerate(rows):
        yy = y + 4 + k
        if k == 0:
            s.fill(c.x0, yy, c.x1, BAR_SEL)
            s.put(c.x0 + 1, yy, ">", St(fg=VIOLET, bg=BAR_SEL))
            x = s.put(c.x0 + 3, yy, label, St(fg=TEXT, bg=BAR_SEL))
            if not c.narrow:
                s.put(c.x0 + 30, yy, desc, St(fg=DIM, bg=BAR_SEL))
        else:
            x = s.put(c.x0 + 3, yy, label, S)
            if not c.narrow:
                s.put(c.x0 + 30, yy, desc, S_DIM)
    footer(s, c, [("↑↓", "select"), ("Enter", "confirm"), ("Esc", "quit")], y=y + 7)


def board_login_waiting(s, c):
    y = 1
    s.put(c.x0 + 1, y, "Welcome to Cortex CLI!", S_BOLD)
    s.spans(c.x0 + 1, y + 2, [("⠇ ", S_DIM), ("Waiting for browser authentication…", S)])
    s.spans(c.x0 + 1, y + 4, [("Your code  ", S_DIM), ("WXYZ-1234", S_BOLD)])
    s.put(c.x0 + 1, y + 5, clip("If the browser didn't open, visit cortex.foundation/cli/auth and enter the code.", c.inner_w - 2), S_DIM)
    footer(s, c, [("Esc", "cancel")], y=y + 7)


def board_login_success(s, c):
    y = 1
    s.put(c.x0 + 1, y, "Welcome to Cortex CLI!", S_BOLD)
    s.spans(c.x0 + 1, y + 2, [("✓ ", S_OK), ("Signed in as ", S), ("mathis@cortex.foundation", S_BOLD)])
    s.spans(c.x0 + 1, y + 4, [("Run ", S_DIM), ("cortex", S), (" to start a session.", S_DIM)])


def board_login_error(s, c):
    y = 1
    s.put(c.x0 + 1, y, "Welcome to Cortex CLI!", S_BOLD)
    s.spans(c.x0 + 1, y + 2, [("× ", S_ERR), ("Sign-in didn't complete", S_ERR)])
    s.put(c.x0 + 1, y + 3, clip("Your Cortex API key may be invalid, expired, or revoked.", c.inner_w - 2), S_DIM)
    s.put(c.x0 + 1, y + 4, clip("Run 'cortex login' or set CORTEX_API_KEY, then try again.", c.inner_w - 2), S_DIM)
    footer(s, c, [("Enter", "try again"), ("Esc", "quit")], y=y + 6)


def board_shortcuts_overlay(s, c):
    settings_backdrop(s, c)
    if c.narrow:
        x, y, w, h = modal(s, c, "Shortcuts")
        rows = [("Shift+Tab", "cycle Agent / Plan / Ask"), ("@", "mention files"), ("!", "bash mode"), ("&", "hand off to Cortex Cloud"), ("Ctrl+c", "stop"), ("F2", "settings"), ("Esc", "interrupt / close")]
        for k, (key, label) in enumerate(rows):
            s.spans(x + 2, y + 2 + k, [(key.ljust(10), S), (label, S_DIM)], max_x=x + w - 2)
        legend(s, y + h - 2, x + 1, x + w - 1, [("Ctrl+x/Esc", "close")])
        return
    x, y, w, h = modal(s, c, "Shortcuts", w=min(84, c.cols - 4), h=14)
    left = [
        ("Shift+Tab", "cycle Agent / Plan / Ask"),
        ("@", "mention files"),
        ("!", "bash mode"),
        ("&", "hand off to Cortex Cloud"),
        ("/", "slash commands"),
        ("Alt+Enter", "newline in the composer"),
        ("↑ / ↓", "edit last · browse history"),
    ]
    right = [
        ("Ctrl+p", "command palette"),
        ("Ctrl+r", "search past sessions"),
        ("Ctrl+c", "stop the current turn"),
        ("Esc", "interrupt · close"),
        ("F2", "settings"),
        ("PgUp / PgDn", "scroll the transcript"),
        ("Ctrl+x", "this overlay"),
    ]
    for k in range(len(left)):
        yy = y + 2 + k
        s.spans(x + 3, yy, [(left[k][0].ljust(12), S), (left[k][1], S_DIM)])
        s.spans(x + w // 2 + 1, yy, [(right[k][0].ljust(13), S), (right[k][1], S_DIM)])
    s.hline(y + 2 + len(left) + 1, x + 2, x + w - 2, HAIR)
    s.center_spans(y + 2 + len(left) + 2, [("Docs & guides: cortex.foundation/docs · Cortex CLI v" + VERSION, S_MUTED)], x + 1, x + w - 1)
    legend(s, y + h - 2, x + 1, x + w - 1, [("Ctrl+x/Esc", "close")])


def board_resume_picker(s, c):
    header(s, c)
    top = composer(s, c, content=[("/resume", S_ACC), (" ", S)])
    rows = [
        ([("fix login redirect", S)], "2h ago · 14 messages · cortex/fix-login-redirect"),
        ([("composer chip design", S)], "yesterday · 31 messages"),
        ([("bump version to 0.1.7", S)], "3 days ago · 6 messages"),
    ]
    menu_top = menu(s, c, top, rows, focused=0, name_w=24)
    search_field(s, c.x0 + 2, menu_top - 3, c.inner_w - 4, placeholder="Type to search sessions")
    backdrop_tail(s, c, menu_top - 3)
    footer(s, c, [("Enter", "resume"), ("f", "favorite"), ("d", "delete"), ("Esc", "close")] if not c.narrow else [("Enter", "resume"), ("Esc", "close")])


def board_clear_confirm(s, c):
    header(s, c)
    top = composer(s, c, content=[("/clear", S_ACC)], caret=False, focused=False)
    y = top - 5
    backdrop_tail(s, c, y)
    f = Flow(s, c, y=y, limit=top - 1)
    f.line([("Clear this conversation?", S)])
    f.dim("The transcript is dropped. Git, files and config stay as they are.")
    f.options(["Yes, clear", "No, keep it"], focused=0)
    footer(s, c, [("Enter", "confirm"), ("Esc", "cancel")])


def board_plan_confirm(s, c):
    header(s, c)
    top = composer(s, c, mode="Plan", focused=False, caret=False, placeholder="Choose an option above")
    f = Flow(s, c, limit=top - 1)
    f.user("add retry with backoff to the api client", "11:20 AM")
    f.thought("Thought for 5.8s")
    f.reply(
        [
            "**Plan** — no files change until you approve.",
            "",
            "1. Wrap `ApiClient::send` in a retry loop: 3 attempts, exponential backoff with jitter.",
            "2. Retry only on timeouts and 5xx; never on 4xx.",
            "3. Surface the final failure as product copy: 'The coding service is temporarily unavailable'.",
            "4. Unit tests for the backoff schedule and the give-up path.",
        ],
        "11:21 AM",
    )
    f.line([("Implement this plan?", S)])
    f.options(["Yes, switch to Agent mode and implement", "No, keep planning — tell Cortex what to change"], focused=0)
    footer(s, c, [("Enter", "confirm"), ("Esc", "keep planning")])


def board_queue(s, c):
    header(s, c)
    top = composer(s, c, placeholder="Add a follow-up — Enter to queue", focused=True)
    f = Flow(s, c, limit=top - 4)
    f.user("run the tui tests and fix whatever fails", "09:14 AM")
    f.thought("Thought for 2.1s")
    f.tile("Shell", "$ cargo test -p cortex-tui", "running · 8s", glyph="⠇", glyph_st=S)
    y = top - 3
    s.spans(c.x0 + 2, y, [("Queued · 2", S), (" — sent when the current step finishes", S_DIM)], max_x=c.x1)
    s.spans(c.x0 + 4, y + 1, [("1  ", S_DIM), ("also update the README screenshots", S)], max_x=c.x1)
    s.spans(c.x0 + 4, y + 2, [("2  ", S_DIM), ("run the full test suite before you finish", S)], max_x=c.x1)
    footer(s, c, [("Enter", "queue"), ("↑", "edit queued"), ("Ctrl+c", "stop"), ("Ctrl+x", "shortcuts")] if not c.narrow else [("Enter", "queue"), ("Ctrl+c", "stop")])


def board_files_picker(s, c):
    header(s, c)
    top = composer(s, c, content=[("explain ", S), ("@comp", S_ACC)])
    paths = [
        ("src/cortex-tui/src/composer.rs", "212 lines"),
        ("src/cortex-tui/src/tests/composer.rs", "88 lines"),
        ("src/cortex-core/src/components/mod.rs", "1.4k lines"),
        ("docs/guides/composer.md", "61 lines"),
    ]
    rows = []
    for path, meta in paths:
        k = path.lower().find("comp")
        rows.append(([(path[:k], S), (path[k : k + 4], S_ACC), (path[k + 4 :], S)], meta))
    menu_top = menu(s, c, top, rows, focused=0, hover=2, name_w=40)
    backdrop_tail(s, c, menu_top)
    footer(s, c, [("Enter", "attach"), ("Tab", "complete"), ("Esc", "close")])


def board_jobs(s, c):
    header(s, c)
    top = composer(s, c, content=[("/jobs", S_ACC), (" ", S)])
    rows = [
        ([("⠇ ", S), ("cloud · fix login redirect", S)], "running · 4m · cortex/fix-login-redirect"),
        ([("✓ ", S_OK), ("subagent · explore test layout", S)], "done · 1m 12s · 3 files read"),
        ([("○ ", S_DIM), ("queued · refresh README screenshots", S)], "starts when the current step finishes"),
    ]
    menu_top = menu(s, c, top, rows, focused=0, name_w=38)
    s.spans(c.x0 + 2, menu_top - 2, [("Jobs", S), (" · 1 running · 1 queued", S_DIM)], max_x=c.x1)
    backdrop_tail(s, c, menu_top - 2)
    footer(s, c, [("Enter", "open"), ("x", "cancel"), ("Esc", "close")])


def board_skills(s, c):
    header(s, c)
    top = composer(s, c, content=[("/skills", S_ACC), (" ", S)])
    rows = [
        ([("release-notes", S)], "Draft release notes from the git log · project"),
        ([("pdf-report", S)], "Render a PDF from a markdown report · user"),
        ([("db-migrate", S)], "Write and verify a schema migration · project"),
    ]
    menu_top = menu(s, c, top, rows, focused=0, name_w=18)
    search_field(s, c.x0 + 2, menu_top - 3, c.inner_w - 4, placeholder="Type to search skills")
    backdrop_tail(s, c, menu_top - 3)
    footer(s, c, [("Enter", "run"), ("r", "reload"), ("Esc", "close")])


def board_todos(s, c):
    header(s, c)
    top = composer(s, c, placeholder="Add a follow-up — Enter to queue", focused=False, caret=False)
    f = Flow(s, c, limit=top - 1)
    f.user("move the model chip into the composer border", "04:11 PM")
    f.thought("Thought for 2.6s")
    f.line([("⠇", S), (" Working 2/5", S), ("  ·  38s · 6.1k tokens", S_DIM)])
    f.line([("✓ ", S_OK), ("Read composer.rs and footer.rs", S_DIM)], indent=2)
    f.line([("✓ ", S_OK), ("Locate the chip painter", S_DIM)], indent=2)
    f.line([("› ", S), ("Move the chip into the bottom hairline", S)], indent=2)
    f.line([("○ ", S_DIM), ("Drop the model from the footer", S_DIM)], indent=2)
    f.line([("○ ", S_DIM), ("Run the TUI snapshot tests", S_DIM)], indent=2)
    footer(s, c, [("Esc", "interrupt"), ("Enter", "queue follow-up"), ("Ctrl+x", "shortcuts")])


def board_question(s, c):
    header(s, c)
    top = composer(s, c, focused=False, caret=False, placeholder="Choose an option above, or type your own answer")
    f = Flow(s, c, limit=top - 1)
    f.user("add retry with backoff to the api client", "11:20 AM")
    f.thought("Thought for 1.9s")
    f.line([("●", S_DIM), (" Which failures should be retried?", S)])
    f.options(["Timeouts and 5xx only (recommended)", "Every network error", "Let me decide per call site"], focused=0)
    footer(s, c, [("↑↓", "select"), ("Enter", "answer"), ("Esc", "skip")])


def board_sudo(s, c):
    header(s, c)
    top = composer(s, c, focused=False, caret=False, placeholder="Enter the password above")
    f = Flow(s, c, limit=top - 1)
    f.user("install the system deps", "02:40 PM")
    f.thought("Thought for 0.7s")
    f.tile("Shell", "$ sudo apt-get install -y libssl-dev")
    f.command("Password for mathis: ••••••••")
    s.caret = (f.tx + len("Password for mathis: ••••••••"), f.y - 1)
    f.dim("Never stored, logged, or shown to the model — sent straight to sudo.", indent=2)
    footer(s, c, [("Enter", "submit"), ("Esc", "cancel")])


def board_config_tree(s, c):
    header(s, c)
    top = composer(s, c, content=[("/config", S_ACC)], caret=False)
    f = Flow(s, c, limit=top - 1)
    f.user("/config", "05:30 PM")
    f.line([("~/.cortex/config.json", S), ("  ·  read-only view · edit with your editor", S_DIM)])
    g = S_DIM
    rows = [
        ("model", "cortex-1-mini"),
        ("effort", "medium"),
        ("permissions", "smart"),
        ("sandbox", "workspace"),
        ("tui.alternate_screen", "true"),
        ("tui.theme.name", "cortex-night"),
        ("tui.animations", "true"),
        ("mcp.servers", "github · filesystem · linear · sentry"),
    ]
    for k, v in rows:
        f.line([(k.ljust(24), g), (v, S)], indent=2)
    footer(s, c)


def board_btw(s, c):
    header(s, c)
    top = composer(s, c)
    f = Flow(s, c, limit=top - 1)
    f.user("run the tui tests and fix whatever fails", "09:14 AM")
    f.thought("Thought for 2.1s")
    f.tile("Shell", "$ cargo test -p cortex-tui", "running · 8s", glyph="⠇", glyph_st=S)
    f.blank()
    f.user("/btw what is the difference between Plan and Ask?", "09:15 AM")
    f.line([("♦", S_DIM), (" Side note", S_DIM), ("  ·  answered without touching the running turn", S_MUTED)])
    f.blank()
    f.reply(["**Plan** drafts an approach and waits for approval before editing. **Ask** never edits — it only answers questions about the codebase."], "09:15 AM")
    footer(s, c, [("Esc", "interrupt"), ("Enter", "queue follow-up"), ("Ctrl+x", "shortcuts")])


# --------------------------------------------------------------------------- #
# Registry — (id, painter, meta). ``narrow`` = also rendered at 40×12.
# --------------------------------------------------------------------------- #

SECTIONS = {
    "A": "Entry / welcome",
    "B": "Session",
    "C": "Slash + model",
    "D": "Settings",
    "E": "Modes / tools / errors",
}

# (id, painter, narrow_too, section, description)
BOARDS_META = [
    # A. Entry / welcome
    ("welcome-cortex", board_welcome_cortex, True, "A", "Cold start `cortex` — clean welcome, alternate screen, no shell echoes"),
    ("welcome-agent", board_welcome_agent, True, "A", "Cold start `agent` — same chrome, agent wording + placeholder"),
    ("first-run-tips", board_first_run, True, "A", "First launch — charcoal tips panel under the welcome"),
    ("session-empty", board_session_empty, True, "A", "Resumed / after first turn — header, composer, footer only"),
    # B. Session
    ("session-user-bars", board_session_user_bars, True, "B", "Two user prompt bars + timestamps, Thought, reply, Worked, ▼ more, opt-in banner"),
    ("session-thought", board_session_thought, False, "B", "`♦ Thought for Xs` collapsed"),
    ("session-thought-expanded", board_session_thought_expanded, False, "B", "Thought expanded (Show thinking blocks = on)"),
    ("session-thinking-live", board_session_thinking_live, True, "B", "Live `⠇ Thinking · 3s` while the turn runs"),
    ("session-assistant", board_session_assistant, True, "B", "Plain reply with bold + bullets"),
    ("session-worked", board_session_worked, False, "B", "`Worked for Xs` after a reply"),
    ("session-optin", board_session_optin, True, "B", "`Help improve Cortex` banner — Opt out | Opt in"),
    ("session-optin-hover", board_session_optin_hover, False, "B", "Banner with the mouse over `[Opt in]`"),
    ("composer-empty", board_composer_empty, True, "B", "Empty composer — caret before the placeholder, violet `>`"),
    ("composer-typing", board_composer_typing, True, "B", "Mid-type, caret on"),
    ("composer-typing-blink", board_composer_typing_blink, False, "B", "Mid-type, caret off (blink phase)"),
    ("composer-hover", board_composer_hover, True, "B", "Mouse over the composer — hairline lifts to #525252"),
    ("composer-multiline", board_composer_multiline, False, "B", "Alt+Enter newlines — box grows upward"),
    ("footer-shortcuts", board_footer_shortcuts, False, "B", "Footer strip with text typed (4 hints)"),
    ("footer-hover", board_footer_hover, False, "B", "Mouse over `Ctrl+x:shortcuts`"),
    ("tokens-topright", board_tokens_topright, True, "B", "Token counter `142K / 500K` top-right"),
    ("tokens-topright-warn", board_tokens_topright_warn, False, "B", "Counter ≥ 90 % — amber + /compact hint"),
    ("compact-chat", board_compact_chat, True, "B", "Compact mode — edge-to-edge bars, no timestamps"),
    # C. Slash + model
    ("slash-palette", board_slash_palette, True, "C", "`/` palette — focused row + hover row + `… more` trailer"),
    ("slash-model-typed", board_slash_model_typed, True, "C", "`/mod` typed — violet matched chars, ghost completion"),
    ("model-list", board_model_list, True, "C", "`/model` — Cortex Mini 1 · Cortex 1 · Cortex Max 1"),
    ("model-list-hover", board_model_list_hover, False, "C", "Model list with mouse over row 3"),
    ("model-effort-high", board_model_effort_high, True, "C", "Effort radios — High focused"),
    ("model-effort-medium", board_model_effort_medium, False, "C", "Effort radios — Medium focused"),
    ("model-effort-low", board_model_effort_low, False, "C", "Effort radios — Low focused"),
    ("model-effort-hover", board_model_effort_hover, False, "C", "Effort radios — Medium focused, mouse over Low"),
    # D. Settings
    ("settings-appearance", board_settings_appearance, True, "D", "Settings modal — Appearance, Compact mode focused"),
    ("settings-mouse", board_settings_mouse, True, "D", "Settings scrolled to Mouse / Behavior"),
    ("settings-row-hover", board_settings_row_hover, True, "D", "Keyboard focus on Compact mode, mouse over Show timestamps"),
    ("settings-search", board_settings_search, False, "D", "`/ scro` search — filtered rows, violet match"),
    ("settings-theme-submenu", board_settings_theme_submenu, True, "D", "Theme submenu — Cortex Night / Cortex Day / Ocean Dark / Monokai"),
    # E. Modes / tools / errors
    ("mode-agent", board_mode_agent, False, "E", "Agent mode — dim chip in the composer border"),
    ("mode-plan", board_mode_plan, True, "E", "Plan mode — `Plan · no edits` chip, plan reply"),
    ("mode-ask", board_mode_ask, True, "E", "Ask mode — `Ask · read-only` chip"),
    ("mode-bash", board_mode_bash, False, "E", "Bash mode (`!`) — chip + `!` sigil"),
    ("permission-prompt", board_permission_prompt, True, "E", "Exec approval — command on gray, numbered radios"),
    ("permission-prompt-hover", board_permission_prompt_hover, False, "E", "Exec approval with mouse over option 2"),
    ("permissions-picker", board_permissions_picker, False, "E", "`/permissions` — Smart / Read-only / Full access"),
    ("mcp-servers", board_mcp_servers, True, "E", "`/mcp` — connected ✓, authenticating, failed ×"),
    ("mcp-drop", board_mcp_drop, False, "E", "MCP server dropped mid-turn (error red)"),
    ("plugins", board_plugins, False, "E", "`/plugins` — enabled / disabled rows"),
    ("usage", board_usage, True, "E", "`/usage` — plan bars"),
    ("quota-exhausted", board_quota_exhausted, False, "E", "Agent quota exhausted — red title, held composer"),
    ("sandbox", board_sandbox, False, "E", "`/sandbox` — filesystem / network / escalation"),
    ("sandbox-deny", board_sandbox_deny, False, "E", "Sandbox blocked a command — red title, radios"),
    ("cloud-handoff", board_cloud_handoff, False, "E", "`&` handoff to Cortex Cloud"),
    ("diagnostics", board_diagnostics, True, "E", "Diagnostics tile — error red, warn amber"),
    ("interrupt-stopped", board_interrupt_stopped, True, "E", "Esc / Ctrl+c — `× Stopped`"),
    ("error-unavailable", board_error_unavailable, False, "E", "API down — product-facing error"),
    ("tool-tiles", board_tool_tiles, False, "E", "Grouped tool calls expanded — Read / Grep / Shell"),
    ("tool-tiles-collapsed", board_tool_tiles_collapsed, False, "E", "Grouped tool calls collapsed"),
    ("shell-running", board_shell_running, False, "E", "Live Shell tile with output"),
    ("diff-hunk", board_diff_hunk, True, "E", "Edit tile + unified hunk — green +, red −"),
    ("edit-collapsed", board_edit_collapsed, False, "E", "Collapsed edit blocks (setting on)"),
    ("md-table", board_md_table, False, "E", "Markdown table — gray plus-ASCII grid"),
    ("code-fence", board_code_fence, False, "E", "Fenced code — language tag hairline, gutter, bold keywords"),
    ("login", board_login, True, "E", "`cortex login` — inline picker"),
    ("login-waiting", board_login_waiting, False, "E", "Waiting for browser + device code"),
    ("login-success", board_login_success, False, "E", "`✓ Signed in`"),
    ("login-error", board_login_error, False, "E", "Sign-in failed — product copy"),
    ("shortcuts-overlay", board_shortcuts_overlay, True, "E", "Ctrl+x shortcuts overlay"),
    ("resume-picker", board_resume_picker, False, "E", "`/resume` — search field + session rows"),
    ("clear-confirm", board_clear_confirm, False, "E", "`/clear` confirm radios"),
    ("plan-confirm", board_plan_confirm, False, "E", "`Implement this plan?` radios"),
    ("queue", board_queue, False, "E", "Follow-up queue while a step runs"),
    ("files-picker", board_files_picker, False, "E", "`@` file picker — violet matched chars, hover row"),
    ("jobs", board_jobs, False, "E", "`/jobs` — cloud agent, subagent, queued"),
    ("skills", board_skills, False, "E", "`/skills` — search field + skill rows"),
    ("todos", board_todos, False, "E", "Working 2/5 checklist — ✓ done · › current · ○ pending"),
    ("question", board_question, False, "E", "Clarifying question radios"),
    ("sudo", board_sudo, False, "E", "Elevated Shell — password row on gray"),
    ("config-tree", board_config_tree, False, "E", "`/config` read-only key tree"),
    ("btw", board_btw, False, "E", "`/btw` side note during a running turn"),
]

BOARDS = [(bid, fn, {"narrow": narrow, "section": sec, "desc": desc}) for bid, fn, narrow, sec, desc in BOARDS_META]
