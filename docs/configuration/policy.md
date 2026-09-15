# Organization policy

Cortex reads a managed policy document for organizations that need to pin
behavior for every member. The document lives in one directory, named by
`CORTEX_ORG_POLICY_DIR`, and is called `policy.json`.

```bash
export CORTEX_ORG_POLICY_DIR=/etc/cortex/policy
```

```json
{
  "fast_mode": false,
  "plugin_install": "require_accept_command"
}
```

Both keys are optional. A missing document, or a document without a key, leaves
that key on the host default.

## Resolution rules

Every key resolves **fail-closed**. A document that exists but cannot be read,
cannot be parsed, or carries a value that is not recognized denies the
restricted behavior. Policy failures never grant a permission.

| Situation | `fast_mode` | `plugin_install` |
|---|---|---|
| No directory, or no `policy.json` | host default | host default |
| Key absent | host default | host default |
| Key `true`, `"on"`, `"enabled"`, `"allowed"` | allowed | optional |
| Key `false`, `"off"`, any other value | disabled | required |
| Document unreadable or unparseable | disabled | required |

## `fast_mode`

When `fast_mode` is `false`, members cannot turn fast mode on.

- `/fast` and `/fast on` show `Fast mode is disabled for your organization.
  Contact your admin.` followed by `Staying on Standard.`
- The session keeps its current model and settings. Nothing is re-sent.
- There is no client flag that bypasses the policy.
- Turning fast mode **off** is always allowed.
- A remote session on Standard shows `Remote · Standard`. The Fast chip appears
  only while fast mode is actually on.

`/fast off` is unaffected by policy, so a member can always return to Standard.

## `plugin_install`

When `plugin_install` is `require_accept_command`, `cortex plugin install` and
`cortex plugin update` refuse to run without `--accept-command`:

```
This organization requires --accept-command for plugin installs.
Run `cortex plugin install <id> --json` to review the commands,
then pass --accept-command <sha256>.
```

The pin itself is enforced the same way for every organization, so a review
that does not match the package fails closed whether or not a policy document
exists. See [Plugins](../customization/plugins.md#pinned-command-installs).

## Audit journal

Fail-closed decisions are appended to `{cortex_home}/audit/events.jsonl`, one
JSON object per line:

```json
{"schema":1,"ts":"2026-09-14T09:26:11Z","kind":"plugin_command_accepted","detail":{"plugin":"cortex-review","version":"1.2.0","accepted_hash":"8f4c…","actual_hash":"8f4c…","action":"install","policy":"require_accept_command"}}
```

| `kind` | Written when |
|---|---|
| `plugin_command_accepted` | A reviewed command hash was accepted for an install or update |
| `instructions_omitted` | A subagent skipped user, project, or local instruction documents |
| `managed_policy_never_omitted` | A request named managed policy; it loaded anyway |

Records carry the plugin or source, the hashes, and the scope names. They never
carry prompt text, file bodies, or secrets.

## See also

- [Environment variables](../configuration/env.md)
- [Plugins](../customization/plugins.md)
- [Agents](../customization/agents.md#omitting-instruction-documents)
