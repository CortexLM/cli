# Getting started

This page takes you from nothing installed to a working Cortex Code session.

## 1. Install

### Linux and macOS

The install script fetches the release build for your platform from
[software.cortex.foundation](https://software.cortex.foundation):

```bash
curl -fsSL https://software.cortex.foundation/install.sh | sh
```

Read it first if you would rather not pipe a script into a shell:

```bash
curl -fsSL https://software.cortex.foundation/install.sh | less
```

### Windows

```powershell
irm https://software.cortex.foundation/install.ps1 | iex
```

### Homebrew and WinGet

The release pipeline includes `homebrew.yml` and `winget.yml` workflows, so
tagged releases can publish a Homebrew formula to `CortexLM/homebrew-tap` and a
WinGet manifest. Use those channels once a release has been cut; the install
script above always works.

### From source

You need the toolchain pinned in [`rust-toolchain.toml`](../../rust-toolchain.toml).

```bash
# Linux only: headers for the optional audio/desktop crates
sudo apt-get install -y libasound2-dev pkg-config

cargo build -p cortex-cli --release
# binary: target/release/Cortex
```

### Check it worked

```bash
cortex --version
```

## 2. Sign in

```bash
cortex login
```

This starts device login against
[api.cortex.foundation](https://api.cortex.foundation) and opens the
verification URL the API returns. On success the session is written to your OS
keyring, not to a file in the repo.

Other ways in, for machines without a browser:

```bash
cortex login --device-auth      # device-code flow
cortex login --sso              # enterprise SSO
cortex login --with-api-key     # read an API key from stdin
cortex login --token "$TOKEN"   # pass a token directly, for CI
```

Confirm and inspect:

```bash
cortex whoami
cortex login status
```

See [Signing in](../reference/login.md) for the full picture, including how
credentials are stored and how to sign out.

## 3. Your first session

Change into a project and start the TUI:

```bash
cd ~/code/my-project
cortex
```

You get the session view from the recording on the [docs index](../README.md):
a timeline, a composer at the bottom, and a status line showing the current mode
and autonomy level. The welcome card shows the working directory and
**Computer** (`This PC` when you started in a workspace, `Cloud` or `SSH` when
those are configured). Type what you want changed and press `Enter`.

Turns go to the Code session API (`POST /v1/code/sessions/{id}/turns`) with
streaming tokens and first-class tool rows. Press `Esc` to cancel a turn that
is going the wrong way. Plan and Spec modes lock mutating tools in the harness
until you switch back to Build.

```
> add a /healthz endpoint and cover it with a test
```

Cortex works through the request as a series of tool calls — searching, reading,
editing and running commands — and each one appears in the timeline as it
happens.

You can also seed the session with a prompt from the command line:

```bash
cortex "explain this repository"
```

The TUI requires a terminal on both stdin and stdout. In a pipeline or CI job,
use [exec mode](exec.md) instead.

## 4. Tell Cortex about your project

`cortex init` writes an `AGENTS.md` in the current directory. Cortex reads it at
the start of a session, so it is the right place for build commands, test
commands, house style and anything a new contributor would need to be told.

```bash
cortex init
```

## 5. Choose how much autonomy to grant

Every session runs under an approval policy and a sandbox policy. By default
Cortex asks before it does anything consequential.

Press `Shift+Tab` in the TUI to cycle autonomy, or set it up front:

```bash
cortex --ask-for-approval on-request   # ask when the agent requests it (default)
cortex --ask-for-approval never        # never prompt
cortex --sandbox read-only             # no writes at all
cortex --sandbox workspace-write       # writes confined to the workspace
cortex --full-auto                     # automatic, inside the sandbox
```

The exact values and what each one permits are in
[Configuration files](../configuration/config.md#permissions-and-sandboxing).

## 6. Where to go next

- [The TUI](tui.md) — everything on screen and how to drive it
- [Sessions](sessions.md) — resuming, exporting and sharing your work
- [Headless / exec mode](exec.md) — the same agent in scripts and CI
- [CLI reference](../reference/cli.md) — every command and flag
- [Troubleshooting](../troubleshooting.md) — when something does not work
