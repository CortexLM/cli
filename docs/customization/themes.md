# Themes

The TUI ships four themes. They set every colour the interface uses — timeline
text, tool rows, borders, status and accents.

| Theme id | Name | Description |
|----------|------|-------------|
| `dark` | Dark | The default. Dark background with green accents. |
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
| Primary accent | `#00FFA3` |
| Secondary accent | `#64FFB4` |
| Links | `#00C882` |
| Background | `#0A1628` |
| Surfaces | `#0D1B2A`, `#1B2838`, `#243B53`, `#334E68` |
| Text | `#FFFFFF` |
| Dim text | `#829AB1` |
| Muted text | `#486581` |
| Border | `#1B4965` |
| Focused border | `#00FFA3` |
| Success | `#00F5D4` |
| Warning | `#FFC857` |
| Error | `#FF6B6B` |
| Info | `#48CAE4` |

Operation modes have their own accent: Build `#00FFA3`, Plan `#FFC857`, Spec
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
