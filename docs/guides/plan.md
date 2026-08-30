# Plan and Spec modes

Sometimes you want the agent to work out *what* it would do before it does
anything. Cortex has two read-first modes for that.

## Plan mode

Plan mode is read-only. The agent can search, read and reason about the project,
but it cannot write files or run commands that change anything. The live Code
API accepts turn `mode` values `chat` and `code` only; Plan and Spec are
harness locks in the CLI (mutating tools stay blocked until you return to
Build).

Reach it from the TUI by cycling the operation mode until the indicator reads
`PLAN`. The indicator sits at the bottom right of the session view; see
[The TUI](tui.md#modes).

Plan mode is also available as a built-in agent, so you can pick it for a single
run:

```bash
cortex run --agent plan "how should we restructure the storage layer?"
```

Use it when you are exploring an unfamiliar area, estimating a change, or you
simply do not want anything touched yet.

## Spec mode

Spec mode goes a step further: the agent produces a structured plan and the
tools that would change your project stay locked until that plan is accepted.

Turn it on in the TUI:

```
/spec        # enter specification mode
/spec off    # leave it
```

Or start a headless run in it:

```bash
cortex exec --use-spec "add rate limiting to the public API"
cortex exec --use-spec --spec-model <model-id> "add rate limiting to the public API"
```

### What the agent produces

In spec mode the agent calls the `Plan` tool, which submits a structured plan for
you to review rather than a wall of prose. A plan carries a title, a description,
the tasks it intends to carry out with a complexity rating on each, and an
analysis from each agent involved. It can also include the architecture it
assumes, the technology it will use, the risks it sees, and the criteria it would
call success.

Until you accept the plan, the mutating tools stay blocked. The agent leaves spec
mode by calling `ExitSpecMode`, at which point it can start editing and running
commands.

### Why bother

- **Review before change.** You see the shape of the work while it is still cheap
  to redirect.
- **A real artefact.** The plan is structured, so it survives being pasted into
  an issue or a design review.
- **A hard stop, not a promise.** The block on mutating tools is enforced by the
  agent harness, not by asking the model nicely.

## Asking you questions

Independently of these modes, the agent can call the `Questions` tool to put a
short structured form on screen — single choice, multiple choice, free text or a
number — when it needs a decision from you rather than a guess. Answering it is
usually faster than a round trip through prose.

## Delegating investigation

The `Task` tool spawns a child task in one of three roles: `explore` (read-only
investigation), `plan` (produce a plan), or `worker` (carry out a slice of work).
Child tasks cannot spawn further children or talk to you directly, so a
delegated investigation always reports back through its parent.

`/tasks` in the TUI shows running background tasks and their progress.

## See also

- [The TUI](tui.md#modes) — switching modes interactively
- [Tools](../reference/tools.md) — `Plan`, `ExitSpecMode`, `Task`, `Questions`
- [Agents](../customization/agents.md) — the built-in `plan` and `explore` agents
- [Headless / exec mode](exec.md) — `--use-spec` and `--spec-model`
