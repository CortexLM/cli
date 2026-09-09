# Long-horizon persisted goals

`/goal` keeps a long-horizon objective attached to the session. It survives the
end of a turn, context compaction, and TUI resume. The agent keeps planning,
acting, and verifying until the work is **evidence-complete** or you pause,
clear, or hit the budget.

This is Cortex session state, not a second coding provider. Agent mode still
talks to the Cortex API. `/goal` is harness state: persist, continue, and wrap
up.

## Commands

Type these in the TUI composer:

| Command | Effect |
|---------|--------|
| `/goal <objective>` | Create or replace the active goal and start a kickoff turn |
| `/goal` or `/goal status` | Show chip, objective, state, progress, next step, and budget |
| `/goal pause` | Stop auto-continuation. The model cannot pause. |
| `/goal resume` | Resume a paused (or blocked) goal if budget remains |
| `/goal clear` | Delete the persisted goal |

A reserved token is only special when it is the entire argument. `/goal pause
the deploy` and `/goal status the rollout` set an objective; they do not pause
or show status.

Resume of a complete or budget-limited goal is refused. Start a new
`/goal <objective>` or `/goal clear`.

## States

`active` · `paused` · `complete` · `budget_limited` · `blocked`

Composer chip copy is text-only (no radios):

| State | Chip |
|-------|------|
| `active` | `Goal · 2/8` (turns used / budget) |
| `paused` | `Goal · paused` |
| `complete` | `Goal · done` |
| `budget_limited` | `Goal · budget` |
| `blocked` | `Goal · blocked` |

Completion is evidence-based. The model calls `UpdateGoal` with a reason and
at least one of: a **file** path, a **command**, or a **test**. Unknown kinds
and vibes are rejected. Resume reloads `goal.json` and paints the chip. A
corrupt file is moved to `goal.json.corrupt` so the session still opens.

Default budget is **8 turns**. After each finished turn the harness records
usage, then continues if budget remains. Near the limit (one turn left, or 85%
of a token cap), the next continuation asks the agent to wrap up instead of
opening new scope. That last remaining turn still runs.

## Where it is stored

`goal.json` lives next to the session files:

```text
~/.cortex/sessions/{session-id}/goal.json
```

Writes are atomic and fsynced. Resume reloads the file. See [Sessions](sessions.md)
and [Data locations](../configuration/data-locations.md).

## Live smoke (operators)

Unit tests always run. A live `/goal` smoke hits a compatible
`/v1/chat/completions` only when a key is present. **Do not commit keys.**

| Variable | Role |
|----------|------|
| `OPENAI_BASE_URL` or `CORTEX_LLM_BASE_URL` | Base URL. Public default for tests/docs: `http://84.32.63.4:20128/v1` |
| `OPENAI_API_KEY` or `CORTEX_LLM_API_KEY` | Injected by the operator (for example from `~/.cortex-private/judge.key`) |
| `CORTEX_LLM_MODEL` | Model id. Must be `cx/gpt-6-astra` (`gpt-astra` 404s) |

Without a key the live test prints `SKIP:` and the rest of the suite still
passes.
