# Skills

A skill is a bundle of instructions the agent can load on demand. Where an
[agent](agents.md) defines *who* is working, a skill defines *how* a particular
job is done — your deployment checklist, your migration procedure, the way your
team writes commit messages.

Skills are loaded through the `UseSkill` tool, so the agent pulls one in when it
decides the task calls for it, rather than carrying every procedure in its
context all the time.

## Built-in skills

| Name | Covers |
|------|--------|
| `git` | Git operations |
| `code-quality` | Quality and style checks |
| `file-operations` | Working with files safely |
| `debugging` | Systematic debugging |
| `security` | Security review |
| `planning` | Planning work |

## Writing a skill

A skill is a directory containing `SKILL.md`: YAML frontmatter, then the
instructions as markdown.

```markdown
---
name: release-checklist
description: Cut a release of this service, from version bump to announcement
version: 1.0.0
author: platform-team
tags: [release, ops]
args:
  - name: version
    description: The semantic version being released
    required: true
  - name: channel
    description: Release channel
    required: false
    default: stable
tools:
  - Read
  - Grep
  - Execute
---

# Release checklist

1. Confirm `main` is green.
2. Bump the version in `Cargo.toml` and `VERSION`.
3. ...
```

### Frontmatter fields

| Field | Type | Notes |
|-------|------|-------|
| `name` | string | Lowercase letters, digits and hyphens; up to 64 characters |
| `description` | string | Up to 1024 characters. This is what the agent matches against. |
| `args` | list | Each entry has `name`, `description`, and optionally `required` and `default` |
| `tools` | list | Restrict the agent to these tools while the skill is active |
| `version`, `author`, `tags` | | Metadata |

`description` is the field that decides whether a skill ever gets used. Write it
as the situation it applies to, not as a title.

## Where skills are found

Searched in order:

1. Built-in skills
2. `./SKILL.md` in the working directory
3. `<project>/.agents/<name>/SKILL.md`
4. `<project>/.agent/<name>/SKILL.md`
5. `<project>/.cortex/skills/<name>/SKILL.md`
6. `~/.cortex/skills/<name>/SKILL.md`

A `<dir>/<name>.md` file is also accepted, and subdirectories are scanned for
`SKILL.md`.

## Using skills

From the TUI:

```
/skills                       list available skills
/skill <name> [args...]       invoke one directly
/skill-reload                 re-read skills from disk after editing
```

From the CLI:

```bash
cortex debug skill <name>     # show how a skill resolves
```

The agent invokes skills itself through the `UseSkill` tool; the slash commands
are for when you want to force the issue.

## Controlling which skills may run

The `permission.skill` table gates skills by name:

```toml
[permission.skill]
"release-checklist" = "ask"
"*" = "allow"
```

## See also

- [Agents](agents.md)
- [Hooks](hooks.md) — run your own commands on lifecycle events
- [Tools](../reference/tools.md) — `UseSkill`
- [Configuration files](../configuration/config.md#permissions-and-sandboxing)
