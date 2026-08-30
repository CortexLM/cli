# Tools

Tools are what turn a model into a coding agent: the agent decides what to call,
Cortex runs it, and the result goes back into the conversation. Every call shows
up in the [timeline](../guides/tui.md#timeline).

Tool names are case-sensitive and are what you pass to `--enabled-tools` and
`--disabled-tools`, and what you list in an [agent's](../customization/agents.md)
`tools` field.

To see what a given run actually has available:

```bash
cortex exec --list-tools
```

## Reading and searching

| Tool | Parameters | Does |
|------|-----------|------|
| `Read` | `file_path`, `offset`, `limit` | Read a file. `limit` defaults to 2400 lines. |
| `LS` | `directory_path`, `ignorePatterns` | List a directory |
| `Grep` | `pattern`, `path`, `case_insensitive`, `line_numbers`, `context`, `context_before`, `context_after`, `glob_pattern`, `output_mode`, `head_limit` | Regular-expression content search. `output_mode` is `file_paths` or `content`. |
| `Glob` | `patterns`, `folder`, `exclude_patterns` | Find files by glob |
| `SearchFiles` | `pattern`, `path`, `content_pattern` | Fuzzy file search |

## Writing

| Tool | Parameters | Does |
|------|-----------|------|
| `Create` | `file_path`, `content` | Create or overwrite a file |
| `Edit` | `file_path`, `old_str`, `new_str`, `change_all` | Replace text in a file |
| `MultiEdit` | `edits` | Apply several edits atomically |
| `ApplyPatch` | `patch`, `dry_run` | Apply a unified diff |

## Running commands

| Tool | Parameters | Does |
|------|-----------|------|
| `Execute` | `command`, `workdir`, `timeout` | Run a shell command. `command` is an argument array. |

`Execute` is the tool the [sandbox and approval policies](../configuration/config.md#permissions-and-sandboxing)
exist for. The default tool timeout is 900 seconds.

## The web

| Tool | Parameters | Does |
|------|-----------|------|
| `WebSearch` | `query`, `num_results`, `category`, `include_domains`, `exclude_domains`, `use_neural`, `livecrawl`, `type`, `context_max_characters` | Search the web |
| `WebFetch` | `url`, `format`, `timeout` | Fetch a page. `format` is `text`, `markdown` or `html`. |
| `FetchUrl` | `url`, `format`, `timeout` | Fetch a URL |

Web search is enabled with `--search`.

## Language intelligence

| Tool | Parameters | Does |
|------|-----------|------|
| `LspDiagnostics` | `path`, `severity` | Diagnostics from the language server. `severity` is `error`, `warning` or `all`. |
| `LspHover` | `file`, `line`, `column` | Hover information. Positions are 1-based. |
| `LspSymbols` | `query`, `path` | Workspace symbol search |

## Planning and coordination

| Tool | Parameters | Does |
|------|-----------|------|
| `Plan` | `title`, `description`, `tasks`, `agent_analyses`, and optional `architecture`, `tech_stack`, `use_cases`, `risks`, `success_criteria`, `timeline`, `estimated_changes` | Submit a structured plan for approval |
| `ExitSpecMode` | `reason` | Leave specification mode and unlock the mutating tools |
| `Task` | `mode` (`explore`, `plan`, `worker`), `prompt`, `description`, `context`, `await_result` | Delegate to a subagent |
| `ListSubagents` | `include_custom` | List the available subagent types |
| `Questions` | `title`, `questions` | Ask you a structured question. Question types are `single`, `multiple`, `text` and `number`. |

See [Plan and Spec modes](../guides/plan.md).

## Task tracking

| Tool | Parameters | Does |
|------|-----------|------|
| `TodoWrite` | `todos` | Update the session todo list. Each item has `id`, `content`, `status` (`pending`, `in_progress`, `completed`) and `priority` (`high`, `medium`, `low`). |
| `TodoRead` | — | Read the current todo list |

## Skills and batching

| Tool | Parameters | Does |
|------|-----------|------|
| `UseSkill` | `skill` | Load a [skill](../customization/skills.md) into the context |
| `Batch` | `calls`, `timeout_secs`, `tool_timeout_secs` | Run 1–10 tool calls in parallel. Cannot nest `Batch` or call agent tools. |

## Tools from extensions

| Source | Naming |
|--------|--------|
| [MCP servers](../customization/mcp.md) | `mcp__<server>__<tool>` |
| [Plugins](../customization/plugins.md) | `plugin_<slug>` |

## What is available when

The tool set is not fixed. It narrows depending on context:

- **Child tasks** cannot call `Task`, `AskUser`, `Questions` or `send_to_user`.
  Delegated work reports back through its parent instead.
- **Specification mode** blocks the mutating tools until the agent calls
  `ExitSpecMode`.
- **Plan mode** and read-only agents are restricted to the reading tools.
- **`WebSearch`** and deep research are chat-surface tools.
- **`--enabled-tools` and `--disabled-tools`** narrow the set further for a
  single [exec run](../guides/exec.md#choosing-tools).
- **The `permission` table** in `config.toml` can require approval for, or
  outright deny, individual capabilities.

## See also

- [Configuration files](../configuration/config.md#permissions-and-sandboxing)
- [Agents](../customization/agents.md#tool-access)
- [MCP servers](../customization/mcp.md)
