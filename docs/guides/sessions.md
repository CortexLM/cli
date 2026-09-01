# Sessions

Every interactive run is a session: the transcript, the tool calls and the
context Cortex built up along the way. Sessions are written to disk as you go,
so you can close the terminal and pick the work back up later. The CLI also
reuses the server-side Code session id for the current workspace
(`~/.cortex/code-sessions.json`) so turns continue the same coding session.

Where they are stored is covered in
[Data locations](../configuration/data-locations.md).

## Listing sessions

```bash
cortex sessions
```

By default this shows sessions started in the current directory. Narrow or widen
the list:

| Flag | Effect |
|------|--------|
| `--all` | Include sessions from other directories |
| `--days <N>` | Only the last N days |
| `--since <YYYY-MM-DD>` | Only sessions on or after this date |
| `--until <YYYY-MM-DD>` | Only sessions on or before this date |
| `--favorites` | Only sessions you marked as favourites |
| `-s`, `--search <TEXT>` | Match on title or ID |
| `-l`, `--limit <N>` | Cap the number shown |
| `--json` | Machine-readable output |

## Resuming

```bash
cortex resume                 # pick from the recent sessions
cortex resume --last          # jump straight back into the most recent one
cortex resume --pick          # force the interactive picker
cortex resume <SESSION_ID>    # resume a specific session
cortex resume --all           # do not filter the picker by directory
```

Inside the TUI, `/resume [session-id]` does the same thing, and `Ctrl+S` or
`Ctrl+O` opens the sessions modal.

`cortex run --continue` continues the most recent session non-interactively, and
`cortex run --session <ID>` continues a specific one.

## Starting fresh

`/new` starts a new session without leaving the TUI. `/clear` empties the current
conversation but keeps the session.

## Rewinding and forking

Press `Esc` twice in the TUI to open the rewind overlay. Use `←`/`→` to pick an
earlier point in the conversation, then:

- `Enter` to roll back to it
- `f` to fork a new session from it
- `Esc` to cancel

`/rewind [steps]`, `/undo` and `/redo` cover the same ground from the composer,
and `/fork [name]` forks the current session.

## Exporting

From the CLI:

```bash
cortex export                          # most recent session
cortex export <SESSION_ID>
cortex export <SESSION_ID> -o out.json
cortex export <SESSION_ID> -f yaml     # json (default), yaml or csv
cortex export <SESSION_ID> --pretty
```

From the TUI, `/export` opens a format picker offering Markdown, JSON and plain
text, and writes the file to your documents or home directory as
`cortex_<title>_<date>.<ext>`. Markdown exports keep the model's reasoning in a
collapsed `<details>` block when it is present.

## Importing

```bash
cortex import session.json
cortex import https://example.com/session.json
cortex import -              # read from stdin
cortex import session.json --force    # overwrite an existing session
cortex import session.json --resume   # import and open it immediately
```

## Sharing

`cortex run --share` shares the session on completion and prints the URL.
In the TUI, `/share [duration]` creates a link; durations are written like `30d`,
`24h`, `60m`, or `never` for a link that does not expire.

## Protecting sessions from cleanup

Locked sessions survive `cortex compact` and `cortex delete`:

```bash
cortex lock add <SESSION_ID> -r "reference for the migration"
cortex lock list
cortex lock check <SESSION_ID>
cortex lock remove <SESSION_ID>
```

## Deleting and cleaning up

```bash
cortex delete <SESSION_ID>          # asks first
cortex delete <SESSION_ID> --yes    # do not ask

cortex compact status               # what cleanup would reclaim
cortex compact run --dry-run        # preview
cortex compact run                  # compact logs, sessions and history
cortex compact vacuum --session-days 30
cortex compact logs --keep-days 7
```

`cortex cache` manages the model, response and update caches separately, and
`cortex logs` reads and prunes the log files.

## Favourites and titles

In the TUI: `/rename <name>`, `/favorite` and `/unfavorite`. `/session` prints
the current session's details, and `/timeline` shows its timeline.

## See also

- [CLI reference](../reference/cli.md#sessions)
- [Data locations](../configuration/data-locations.md)
- [Slash commands](../reference/slash-commands.md#session)
