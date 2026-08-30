# Keyboard shortcuts

Bindings in the [TUI](../guides/tui.md), grouped by where they apply. Press `?`
or `F1` at any time for the in-app version.

## Global

| Key | Action |
|-----|--------|
| `Ctrl+Q` | Quit |
| `Ctrl+C` | Copy the selection |
| `Ctrl+Shift+C` | Copy the selection |
| `Ctrl+Shift+V` | Paste |
| `?` or `F1` | Help |
| `Esc` | Cancel or close |
| `Tab` | Focus the next element |
| `Shift+Tab` | Cycle the autonomy level |
| `Ctrl+K` or `Ctrl+P` | Command palette |
| `Ctrl+I` | Focus the composer |
| `Ctrl+B` | Toggle the sidebar |
| `Ctrl+N` | New session |
| `Ctrl+S` or `Ctrl+O` | Sessions |
| `Ctrl+M` | Switch model |
| `Ctrl+E` | MCP servers |
| `Ctrl+T` | Transcript |

## Composer

| Key | Action |
|-----|--------|
| `Enter` | Send |
| `Shift+Enter` | Insert a newline |
| `Up` / `Down` | Previous / next prompt in history |
| `Ctrl+U` or `Ctrl+L` | Clear the composer |
| `Ctrl+V` | Paste |
| `Ctrl+A` | Select all |

## Timeline

Applies when the transcript has focus.

| Key | Action |
|-----|--------|
| `j` / `k` | Scroll down / up |
| `Down` / `Up` | Scroll down / up |
| `g` | Jump to the top |
| `Shift+G` | Jump to the bottom |
| `Home` / `End` | Jump to the top / bottom |
| `PageUp` / `PageDown` | Page up / down |
| `Ctrl+U` / `Ctrl+D` | Half page up / down |
| `y` | Copy the selection |
| `p` | Paste |
| `e` | Expand or collapse tool details |
| `Tab` | Cycle the autonomy level |

## Approval prompt

| Key | Action |
|-----|--------|
| `y` or `Enter` | Approve |
| `n` or `Esc` | Reject |
| `s` | Approve for this session |
| `a` | Always allow |
| `Shift+A` | Approve all |
| `Shift+R` | Reject all |
| `d` | View the diff |
| `Ctrl+A` | Show the full diff |

## Session sidebar

| Key | Action |
|-----|--------|
| `Enter` | Load the selected session |
| `j` / `k` or `Down` / `Up` | Move the selection |
| `d` or `Delete` | Delete |
| `r` or `F2` | Rename |
| `e` | Export |

## Rewind overlay

Opened by pressing `Esc` twice in quick succession.

| Key | Action |
|-----|--------|
| `←` / `→` | Move between points in the conversation |
| `Enter` | Roll back to the selected point |
| `f` | Fork a new session from it |
| `Esc` | Cancel |

## Resume picker

Shown at startup when there are sessions to resume.

| Key | Action |
|-----|--------|
| `Enter` | Resume the selected session |
| `F` | Fork from it |
| `N` or `Esc` | Start a new session |

## Modals

| Key | Action |
|-----|--------|
| `↑` / `↓` or `k` / `j` | Move the selection |
| `Enter` | Confirm |
| `Esc` | Close, or clear the search box first |

In the theme picker, moving the selection previews the theme live and `Esc`
reverts to the one you started with.

## While a turn is running

| Key | Action |
|-----|--------|
| `Esc` | Interrupt the turn |
| `Ctrl+C` | Force quit |

## See also

- [The TUI](../guides/tui.md)
- [Slash commands](slash-commands.md)
