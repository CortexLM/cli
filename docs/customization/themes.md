# Themes

The TUI ships four themes. They set every colour the interface uses — timeline
text, tool rows, borders, status and accents.

| Theme id | Name | Description |
|----------|------|-------------|
| `dark` | Dark | The default. Host terminal background with violet accents. |
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
| Primary accent | `#A78BFA` |
| Secondary accent | `#C4B5FD` |
| Links | `#8B5CF6` |
| Background | terminal default (`Color::Reset` — never painted) |
| Surfaces | `#141417`, `#1C1C20`, `#26262B`, `#32323A` |
| Selection bar | `#221A38` (light text, never inverted) |
| Text | `#FFFFFF` |
| Dim text | `#829AB1` |
| Muted text | `#486581` |
| Border | `#2A2A32` |
| Focused border | `#A78BFA` |
| Success | `#A78BFA` |
| Diff additions | `#4ADE80` (the only green in the chrome) |
| Warning | `#FFC857` |
| Error | `#FF6B6B` |
| Info | `#48CAE4` |

Operation modes have their own accent: Build `#A78BFA`, Plan `#FFC857`, Spec
`#8B5CF6`.

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
