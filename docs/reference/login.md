# Signing in

Cortex CLI authenticates against the coding API at
[api.cortex.foundation](https://api.cortex.foundation). Device login uses
`POST /v1/auth/device` and `POST /v1/auth/device/token` on that host. The CLI
prints the verification URL the API returns — do not paste a personal access
token.

## Browser sign-in

The default. Run:

```bash
cortex login
```

Cortex starts the device login flow and opens the verification URL in your
browser. When it completes, the session is written to your OS keyring.

## Device code

For a machine with no browser — a server, a container, a remote shell:

```bash
cortex login --device-auth
```

Cortex prints a code and a URL. Open the URL on a device that does have a
browser, enter the code, and the CLI picks up the session when it is approved.

## Guest session

From the TUI login screen, choose **Guest session** for a limited session
without an account. Account login is never silently replaced with a guest
session.

## Enterprise SSO

```bash
cortex login --sso
```

## API key

For CI and other unattended use. The key is read from stdin so it never appears
in your shell history or in `ps` output:

```bash
cortex login --with-api-key < key.txt
printf '%s' "$CORTEX_API_KEY" | cortex login --with-api-key
```

## Token

```bash
cortex login --token "$CORTEX_AUTH_TOKEN"
```

Prefer taking the value from an environment variable your CI secret store
populates, rather than writing it into a workflow file.

## Environment variables

Set instead of running `cortex login` when a keyring is not available:

| Variable | Purpose |
|----------|---------|
| `CORTEX_API_KEY` | API key |
| `CORTEX_AUTH_TOKEN` | Session or bearer token |
| `CORTEX_API_URL` | API base URL, if you need to point elsewhere |

See [Environment variables](../configuration/env.md#authentication).

## Checking your session

```bash
cortex whoami        # the signed-in account
cortex login status  # the state of the stored session
```

In the TUI, `/account` (aliases `/whoami`, `/me`) shows the same thing, and
`/login` starts the flow without leaving the session.

## Signing out

```bash
cortex logout          # asks first
cortex logout --yes
cortex logout --all    # every stored credential
```

`/logout` does the same from the TUI.

## Where credentials are stored

In the OS keyring, under the service `cortex-cli` with the account `auth`:

| Platform | Store |
|----------|-------|
| macOS | Keychain |
| Linux | Secret Service (GNOME Keyring, KWallet, …) |
| Windows | Credential Manager |

They are deliberately not written to a plaintext file in your home directory,
and never to the repository. Anything under `~/.cortex/auth/` holds non-secret
session material only.

If no keyring is available — a bare container, a headless CI runner — use
`CORTEX_API_KEY` or `CORTEX_AUTH_TOKEN` instead of trying to make one work.

## Troubleshooting

**"The coding service is temporarily unavailable"** — Cortex could not reach the
coding API. Check network access to `api.cortex.foundation`, and whether a proxy
or firewall is in the way. This message is intentionally the whole story: Cortex
does not surface provider, SDK or transport names.

**Sign-in appears to succeed but `cortex whoami` fails** — the keyring probably
did not persist the session. Check that a Secret Service provider is running on
Linux, or fall back to `CORTEX_API_KEY`.

**A CI job cannot sign in** — CI should not use the browser flow. Use
`--with-api-key`, `--token`, or set `CORTEX_API_KEY` in the job environment.
Never commit the value; see [CI secrets](../CI_SECRETS.md).

## See also

- [Getting started](../guides/getting-started.md)
- [Environment variables](../configuration/env.md)
- [Troubleshooting](../troubleshooting.md)
