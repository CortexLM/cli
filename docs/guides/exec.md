# Headless and one-shot runs

Cortex has two non-interactive entry points:

| Command | Use it for |
|---------|-----------|
| `cortex exec` | CI, scripts and automation. Autonomy levels, structured output, turn and time limits. |
| `cortex run` | A single request from your own shell. Streams a formatted answer, can continue a session, can share the result. |

Both work without a terminal, so they are safe in pipelines where the
[TUI](tui.md) refuses to start.

## `cortex exec`

### Giving it a prompt

```bash
cortex exec "explain what src/main.rs does"
cortex exec -f prompt.txt
echo "review the diff" | cortex exec
```

The prompt can come from arguments, `--file`, or stdin.

### Autonomy

`--auto` decides what the agent may do without asking. There is nobody to ask in
headless mode, so this is the safety control that matters.

| Level | What it allows |
|-------|----------------|
| `read-only` | Reading, searching and analysing. No file modifications, no command execution. **Default.** |
| `low` | Basic file operations. Good for documentation updates, formatting and comments. |
| `medium` | Adds package installation, builds and local git operations. |
| `high` | Full access, including operations that reach outside the workspace. |

```bash
cortex exec --auto read-only "review this code for security issues"
cortex exec --auto low      "fix all formatting issues in src/"
cortex exec --auto medium   "implement unit tests for the auth module"
```

`--skip-permissions-unsafe` bypasses every permission check and cannot be
combined with `--auto`.

> **Only use `--skip-permissions-unsafe` in a disposable environment.** An
> isolated container or an ephemeral CI runner with no credentials is fine.
> A developer machine, a shared runner, or anything holding secrets is not.

### Output formats

`-o` / `--output-format`:

| Value | Output |
|-------|--------|
| `text` | Human-readable text. **Default.** |
| `json` | One JSON document with the final result |
| `stream-json` | JSON Lines, one event per line, emitted as execution proceeds |
| `debug` | Deprecated alias for `stream-json` |
| `stream-jsonrpc` | JSON-RPC streaming for multi-turn conversations |

`--input-format` accepts `text` (default) or `stream-jsonrpc`.

`--response-format` shapes what the model returns rather than how the CLI prints
it, and takes `text`, `json` or `json_object`. `--output-schema` takes inline
JSON or a path to a schema file for structured output.

```bash
cortex exec -o json "list all TODO comments" | jq -r '.response'
cortex exec -o stream-json "run the test suite" | tee run.jsonl
```

### Adding context

| Flag | Effect |
|------|--------|
| `--include <GLOB>` | Include matching files (repeatable) |
| `--exclude <GLOB>` | Exclude matching files (repeatable) |
| `--git-diff` | Include the current git diff |
| `--url <URL>` | Fetch a URL and add it to the context (repeatable) |
| `--clipboard` | Read the clipboard into the context |
| `-i`, `--image <PATH>` | Attach an image (repeatable) |

```bash
cortex exec --include "src/**/*.rs" --exclude "**/*_test.rs" "review the source"
cortex exec --git-diff --auto read-only "review my uncommitted changes"
```

### Limits

```bash
cortex exec --timeout 1800 "refactor the storage layer"   # default 600 seconds
cortex exec --max-turns 10 "quick task"                   # default 100 turns
```

A turn is one complete request/response cycle with the model.

### Choosing tools

```bash
cortex exec --list-tools
cortex exec --enabled-tools Read,Grep,Glob "map the module structure"
cortex exec --disabled-tools Execute "suggest a fix without running anything"
```

Tool names are the ones in the [tools reference](../reference/tools.md).

### Other useful flags

| Flag | Effect |
|------|--------|
| `-m`, `--model <MODEL>` | Model for this run |
| `--system <PROMPT>` | Replace the system prompt |
| `--max-tokens <N>` | Cap the response length |
| `-r`, `--reasoning-effort <LEVEL>` | Reasoning effort |
| `--use-spec` / `--spec-model <MODEL>` | Run through [Spec mode](plan.md) |
| `-s`, `--session-id <ID>` | Continue an existing session |
| `--cwd <PATH>` | Working directory |
| `--echo` | Include the prompt in the output |
| `-v`, `--verbose` | Verbose logging |

Run `cortex exec --help` for the complete list.

## `cortex run`

`cortex run` is the interactive-adjacent one-shot: it streams a formatted answer
into your terminal and understands sessions.

```bash
cortex run "explain the release process"
cortex run --continue "now write it up as a checklist"
cortex run --session <ID> "and add the rollback steps"
cortex run --share "summarise today's changes"     # prints a share URL
cortex run --agent reviewer --format json "review src/auth"
cortex run -f context.md -o answer.md "turn this into a runbook"
```

`--format` (aliased as `--output`) accepts `default`, `json` or `jsonl`.
`--copy` puts the final response on the clipboard, `--output-file` writes it to
disk, and `--notification` raises a desktop notification when the run finishes.

## Examples

### GitHub Actions

```yaml
name: Cortex review
on: [pull_request]

jobs:
  review:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - name: Review the diff
        env:
          CORTEX_API_KEY: ${{ secrets.CORTEX_API_KEY }}
        run: |
          cortex exec --auto read-only --git-diff \
            --timeout 600 --max-turns 20 \
            "Review this diff for bugs, security issues and missing tests"
```

`cortex github install` scaffolds workflows for PR review and issue automation
if you would rather not write the YAML yourself.

### GitLab CI

```yaml
code-review:
  script:
    - cortex exec --auto read-only -o json "Review the code changes" > review.json
  artifacts:
    paths: [review.json]
```

### A shell script

```bash
#!/usr/bin/env bash
set -euo pipefail

if git diff --quiet; then
  echo "No changes to review"
  exit 0
fi

cortex exec --auto read-only --git-diff \
  --timeout 300 --max-turns 20 \
  -o json "Review these changes for issues" \
  | jq -r '.response'
```

### Continuing a session across invocations

```bash
session=$(cortex exec -o json "analyse this codebase" | jq -r '.session_id')
cortex exec -s "$session" "now focus on the auth module"
```

## Practices worth keeping

1. **Start at `read-only` and only raise autonomy when the task needs it.** A
   review job never needs write access.
2. **Always set `--timeout` and `--max-turns` in CI.** They are the difference
   between a failed job and a runner that hangs.
3. **Use `-o json` or `-o stream-json` when something downstream parses the
   output.** `text` is for humans and its shape is not a contract.
4. **Keep the transcript.** `-o stream-json … | tee run.jsonl` gives you an audit
   trail when a run does something surprising.
5. **Pass credentials through the environment, never on the command line.** See
   [Environment variables](../configuration/env.md).

## See also

- [CLI reference](../reference/cli.md#cortex-exec)
- [Tools](../reference/tools.md)
- [Plan and Spec modes](plan.md)
- [Troubleshooting](../troubleshooting.md)
