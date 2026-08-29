# Security

- Treat every tool path (read / edit / exec / search) as hostile input. Resolve paths inside the workspace; reject `..` escapes and absolute paths outside the project unless the user explicitly opened that root.
- Exec goes through `cortex-execpolicy` + sandbox. Do not add a bypass "because this is local" or "because CI is trusted."
- Auth tokens live in the OS keyring (`cortex-keyring-store`) or `CORTEX_API_KEY` / `CORTEX_AUTH_TOKEN`. Never write tokens to logs, snapshots, TUI captures, or git.
- HTTP clients talk only to Cortex domains by default (`api.cortex.foundation`, `auth.cortex.foundation`, `software.cortex.foundation`). New egress hosts need an explicit allow and a product reason.
- MCP servers are untrusted. Validate URLs, isolate stdio, and do not auto-run remote install scripts.
- `unsafe` is allowed only in sandbox/seccomp/landlock crates and must stay reviewed. Prefer safe wrappers.
- Secrets for CI are listed in `docs/CI_SECRETS.md`. Do not invent cloud accounts. Do not put secret values in the repo.
- PRs must attest a security review of the diff (auth, exec, path, network, secrets).
