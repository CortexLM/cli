# Getting started

This page takes you from nothing installed to a working Cortex Code session.

## 1. Install

### Linux and macOS

The install script fetches the release build for your platform from
[software.cortex.foundation](https://software.cortex.foundation), verifies the
SHA-256 checksum from the release manifest, and installs into `~/.local/bin`
(`Cortex`, plus `cortex` and `agent` symlinks). Add that directory to `PATH` if
it is not already there. Python 3.8 or later is required. Downloads are bounded,
redirects are rejected, and archive extraction accepts only the release binary.
The installer refuses to overwrite unrelated commands or binary symlinks.

```bash
curl -fsSL https://software.cortex.foundation/install.sh | sh
```

Read it first if you would rather not pipe a script into a shell:

```bash
curl -fsSL https://software.cortex.foundation/install.sh | less
```

Pin a version by passing `CORTEX_VERSION=0.1.8` to the shell that runs the
installer (for example, `... | CORTEX_VERSION=0.1.8 sh`). Set
`CORTEX_INSTALL_DIR` to change the prefix; the executable goes in its `bin`
directory. `CORTEX_CHANNEL` accepts `stable`, `beta`, or `nightly`.

The installer selects GNU or musl Linux assets separately for x86_64 and
AArch64, and macOS assets for Intel and Apple silicon. Unknown architectures,
unknown Linux libc implementations, and missing platform assets fail closed.
Musl release builds disable audio and must pass a static-link check. These
matrix entries do not establish minimum kernel/libc/macOS versions or native
login, sandbox, terminal, and update compatibility; those still need native
acceptance evidence.

### Windows

Windows x64 installs into `%LOCALAPPDATA%\Cortex\bin`. Add that folder to
your user `PATH` yourself; the installer does not edit your profile or PATH.
Native Windows ARM64 and 32-bit builds are not published, and the installer
does not silently substitute another architecture:

```powershell
irm https://software.cortex.foundation/install.ps1 | iex
```

### Homebrew and WinGet

The release pipeline references `cortex-cli-macos-arm64.tar.gz` for its
Homebrew formula in `CortexLM/homebrew-tap`, and `cortex-cli-windows-x64.zip`
for the WinGet package `CortexLM.Cortex`. This does not establish that either
external package is published or installs successfully. The Homebrew workflow
currently addresses only the Apple-silicon artifact; Intel/Linux formula
coverage needs a separately verified tap change.

WinGet publishing requires reviewed `wingetcreate.exe` tooling provisioned on
the Windows runner with a valid Microsoft signature. It fails rather than
executing an unpinned tool downloaded during publication. No release signing,
notarization, provenance attestation, or SBOM infrastructure is supplied here.

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

### Update

```bash
cortex upgrade          # latest on the stable channel
cortex upgrade --check  # report only
```

`cortex upgrade` talks to `https://software.cortex.foundation`
(`/releases/manifest.json` and `/v1/assets/...`) and verifies SHA-256 before
replacement. The CLI retains an adjacent `.old` recovery copy, checks the
installed `--version` with a deadline and bounded output, and restores the copy
if that check fails. Lookup failures return a nonzero exit status; an unavailable
release is not reported as an already-installed success. Version pins use
SemVer ordering, including prereleases.

The CLI refuses automatic musl replacement until the shared selector is
libc-aware; use the verified shell installer for those installations. Detected
package-manager installations are directed to their package manager instead of
running the shared library's unverified delegation command. The shared update
library and other callers still need separate fixes/acceptance for ownership
detection, libc selection, bounded extraction, and Windows deferred replacement.

Re-running the installer stages and checks the target binary's `--version`
before replacement, then checks it again at the installed path. An existing
binary is retained as `Cortex.old` (`Cortex.old.exe` on Windows); a failed
post-install check attempts to restore it. Windows locked-file failures stop
instead of scheduling an unverified deferred replacement. Keep the previous
binary until you have checked the new release in your environment. Interruptions,
filesystem failure, and native Windows/macOS replacement still require platform
acceptance tests.

SHA-256 values come from the same distribution origin as the archive. They
detect corruption, **not independent publisher identity**; checksums are not
signatures. No published asset or third-party package availability is implied
by local fixture tests.

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
and autonomy level. The TUI and `cortex exec` run on the **Cloud** Code runtime
by default (Designer Q9), so a fresh install can complete a turn without extra
configuration. To run tools on This PC or over SSH, set `CORTEX_COMPUTER` (see
[Environment variables](../configuration/env.md)). Those runtimes are explicit
opt-in in 0.1.x and need an already connected Code session; Cortex will not
substitute Cloud. Type what you want changed and press `Enter`.

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
