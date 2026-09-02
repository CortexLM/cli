# Themes

The TUI ships four themes. They set every colour the interface uses — timeline
text, tool rows, borders, status and accents.

| Theme id | Name | Description |
|----------|------|-------------|
| `dark` | Dark | The default. Gray chrome on the host terminal background; the Cortex violet marks the focused selection. |
| `light` | Light | Light background with dark text. |
| `ocean_dark` | Ocean Dark | Deep blue and cyan. Also accepted as `ocean`. |
| `monokai` | Monokai | Classic code-editor colours. |

## Switching

From the TUI:

```
/theme               open the picker
/theme monokai       switch directly
```

In the picker, `↑`/`↓` (or `k`/`j`) previews a theme live, `Enter` accepts and
`Esc` reverts to what you had.

From configuration:

```toml
[tui.theme]
name = "ocean_dark"
```

`/reload-config` picks up a change made on disk without restarting.

## The default palette

For reference — these are the colours the demo recording on the
[docs index](../README.md) uses.

| Role | Colour |
|------|--------|
| Selection accent (`>` caret + focused label only) | `#A78BFA` |
| Background | terminal default (`Color::Reset` — never painted) |
| Charcoal panel (tips / info) | `#141414` |
| Past user turn bar | `#1C1C1C` |
| Selection bar | `#262626` (violet caret + label, dim description, never inverted — never a violet wash) |
| Hairline (above / below the prompt, around search fields) | `#3A3A3A` |
| Focused border | `#525252` (gray — the accent never outlines a box) |
| Text | `#FFFFFF` |
| Dim text (placeholders, hints, descriptions) | `#6B7280` |
| Muted text | `#4B5563` |
| Success `✓` and diff additions `+N` | `#4ADE80` (the only green) |
| Warning (`warn` in diagnostics) | `#FFC857` |
| Error (`error` in diagnostics) | `#FF6B6B` |
| Thinking status | `#C9A95C` (the only gold) |

The footer is gray: the model on the left (`Cortex Mini 1 · Agent · 92%
context`), one shortcut hint on the right (`shift+tab to cycle modes`).

## Related display settings

```toml
[tui]
animations = true
notifications = true
```

`/compact` toggles a denser layout, and `--color auto|always|never` controls
colour for non-TUI output.

## See also

- [The TUI](../guides/tui.md)
- [Configuration files](../configuration/config.md#tui)
