# Agents

An agent is a named configuration for how Cortex should behave: which model,
how much reasoning, and which tools it is allowed to touch. Cortex ships a few
and you can add your own.

## Built-in agents

| Name | Kind | Behaviour |
|------|------|-----------|
| `build` | Primary | Full access. The default working agent. |
| `plan` | Primary | Read-only. Investigates and proposes without changing anything. |
| `explore` | Subagent | Read-only investigation, capped at 15 steps. |
| `general` | Subagent | General-purpose worker. Cannot delegate further. |
| `research` | Subagent | Read-only research. |
| `title` | Primary | Names sessions. Uses the small model. |
| `summary` | Primary | Summarises sessions. Uses the small model. |

`title` and `summary` are internal housekeeping; you will not normally select
them by hand.

## Managing agents

```bash
cortex agent list                 # everything available
cortex agent list --primary       # only primary agents
cortex agent list --subagents     # only subagents
cortex agent list --json
cortex agent show <name>
cortex agent create               # interactive
cortex agent create --generate "reviews Rust for concurrency bugs"
cortex agent edit <name>          # opens $EDITOR
cortex agent copy <source> <dest>
cortex agent export <name> -o agent.md
cortex agent install <name>       # from the registry
cortex agent remove <name>
```

In the TUI, `/agents` lists and manages them, and `/delegates` covers subagents.

## Writing an agent

An agent is a markdown file with YAML frontmatter. The body is the agent's
system prompt.

```markdown
---
name: reviewer
description: Reviews changes for correctness and missing tests
model: inherit
reasoning_effort: high
tools: read-only
temperature: 0.2
max_steps: 25
color: cyan
hidden: false
---

You review code changes. Read the diff and the surrounding files, then report
what is wrong, what is missing and what you would change, in that order. Do not
edit anything.
```

### Frontmatter fields

| Field | Type | Notes |
|-------|------|-------|
| `name` | string | How the agent is selected |
| `description` | string | Shown in `cortex agent list` |
| `model` | string | A model id, or `inherit` to use the session's model. Default `inherit`. |
| `reasoning_effort` | `low` \| `medium` \| `high` | |
| `tools` | category or list | See below |
| `temperature` | number | |
| `max_steps` | integer | Cap on tool-calling steps |
| `color` | string | Colour used in the TUI |
| `hidden` | bool | Hide from the default listing |

### Tool access

`tools` takes either a category name or an explicit list of tool names.

| Category | Grants |
|----------|--------|
| `read-only` | `Read`, `LS`, `Grep`, `Glob` |
| `edit` | `Create`, `Edit`, `ApplyPatch` |
| `execute` | `Execute` |
| `web` | `WebSearch`, `FetchUrl` |
| `mcp` | Tools from connected MCP servers |
| `all` | The full built-in set |

An explicit list is exact:

```yaml
tools:
  - Read
  - Grep
  - Glob
  - WebSearch
```

Tool names are the ones in the [tools reference](../reference/tools.md).

## Where agent files are found

Searched in order; the first file defining a given name wins:

1. `<project>/.agents/*.md`
2. `<project>/.agent/*.md`
3. `<project>/.cortex/agents/*.md`
4. `~/.cortex/agents/*.md`
5. `~/.config/cortex/agents/*.md`

Project agents therefore override personal ones, which is what you want when a
repository has house rules.

## Using an agent

```bash
cortex run --agent reviewer "review the last commit"
cortex --config current_agent=reviewer
```

```toml
# config.toml
current_agent = "reviewer"
```

In a prompt, `@name` mentions an agent directly:

```
> @reviewer take a look at src/auth before I push this
```

## Subagents

Subagents are agents the main agent delegates to, through the `Task` tool. A
task runs in one of three roles — `explore`, `plan` or `worker` — and reports
back when it finishes.

Delegated work is constrained: a child task cannot spawn its own children, ask
you questions directly, or message you. Everything flows back through the parent.

Built-in subagent types are `code`, `research`, `refactor`, `test`,
`documentation`, `security`, `architect` and `reviewer`, plus any custom agent
you define.

`/tasks` shows what is running in the background.

## See also

- [Skills](skills.md) — reusable instructions rather than whole agents
- [Plan and Spec modes](../guides/plan.md)
- [Tools](../reference/tools.md)
- [CLI reference](../reference/cli.md#cortex-agent)
