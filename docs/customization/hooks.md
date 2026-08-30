# Hooks

Hooks run your own commands when something happens in a session — a file was
edited, a tool is about to run, the session ended. They are how you wire Cortex
into a formatter, a linter, a notifier, or an audit log.

This page covers **shell hooks**, which run external commands. Plugins can also
register hooks inside the WebAssembly runtime; see
[plugin hooks](plugins/hooks.md).

## Events

| Event | Fires |
|-------|-------|
| `SessionStart` | A session begins |
| `SessionEnd` | A session ends |
| `SessionCompleted` | A session finishes its work |
| `Setup` | Initial setup |
| `UserPromptSubmit` | You submit a prompt |
| `PreToolUse` | Before a tool runs |
| `PostToolUse` | After a tool succeeds |
| `PostToolUseFailure` | After a tool fails |
| `PermissionRequest` | An approval is requested |
| `FileCreated` | The agent created a file |
| `FileEdited` | The agent edited a file |
| `FileDeleted` | The agent deleted a file |
| `SubagentStart` | A subagent starts |
| `SubagentStop` | A subagent stops |
| `PreCompact` | Before the conversation is compacted |
| `Notification` | A notification is raised |
| `Stop` | Execution is stopped |

## Configuration

Hooks are configured as JSON. File-oriented events are keyed by glob pattern;
others take a list of hooks directly.

```json
{
  "file_edited": {
    "*.rs": [
      {
        "command": ["rustfmt", "{file}"],
        "timeout": 30
      }
    ],
    "*.ts": [
      { "command": ["npx", "prettier", "--write", "{file}"] }
    ]
  },
  "session_completed": [
    {
      "command": ["./scripts/notify.sh", "{session_id}"],
      "async_execution": true
    }
  ],
  "formatter": {
    "enabled": true,
    "overrides": {},
    "disabled": []
  }
}
```

### Hook fields

| Field | Type | Meaning |
|-------|------|---------|
| `command` | array of strings | The command and its arguments. Not a shell string — no quoting rules to get wrong. |
| `environment` | object | Extra environment variables for the command |
| `timeout` / `timeout_secs` | integer | Seconds before the hook is killed |
| `async_execution` | bool | Do not block the agent on this hook |
| `once` | bool | Run at most once per session |
| `pattern` | string | Glob the hook applies to |
| `tool_matcher` | string | For `PreToolUse` and `PostToolUse`: pipe-separated tool names, for example `Edit\|Create\|ApplyPatch` |

### Placeholders

| Placeholder | Replaced with |
|-------------|---------------|
| `{file}` | The file the event concerns |
| `{path}` | The path the event concerns |
| `{session_id}` | The current session id |
| `{message_id}` | The current message id |

The same values are also exported to the command as `CORTEX_FILE`,
`CORTEX_SESSION_ID` and `CORTEX_MESSAGE_ID`.

## Outcomes

A hook reports one of `success`, `failure`, `async_started`, `skipped` or
`timeout`. A failing `PreToolUse` hook is how you stop a tool call you do not
want to happen.

## Managing hooks from the TUI

```
/hooks           list and manage file hooks
```

## Worked example: format on edit, block writes to generated files

```json
{
  "file_edited": {
    "*.rs": [{ "command": ["rustfmt", "{file}"], "timeout": 15 }]
  },
  "pre_tool_use": [
    {
      "command": ["./scripts/deny-generated.sh", "{file}"],
      "tool_matcher": "Edit|Create|ApplyPatch",
      "timeout": 5
    }
  ]
}
```

`deny-generated.sh` exits non-zero when the path is generated, which fails the
hook and stops the write.

## See also

- [Plugin hooks](plugins/hooks.md) — the in-process WebAssembly equivalent
- [Skills](skills.md)
- [Configuration files](../configuration/config.md)
