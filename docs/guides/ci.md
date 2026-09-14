# CI cookbook

Run Cortex CLI in a pipeline without a terminal. This is the consolidated
cookbook: how to authenticate from a secret store, which command to use, what
each exit code means, and what to do with the output.

The headless entrypoints are `cortex run` (one message, one result) and
`cortex exec` (one task, richer output and autonomy control). Both are
non-interactive and both fail closed when the coding service is unreachable.

## 1. Authenticate from the environment

Never paste a token into a workflow file. Every supported CI system has a secret
store; put the token there and export it as an environment variable.

| Variable | Use |
|---|---|
| `CORTEX_API_KEY` | API key. Set this in CI. |
| `CORTEX_AUTH_TOKEN` | Session / bearer token. Checked before `CORTEX_API_KEY`. |
| `CORTEX_API_URL` | Override the API origin. Operators and tests only. |

Resolution order is the stored auth file, then `CORTEX_AUTH_TOKEN`, then
`CORTEX_API_KEY`. In CI there is no keyring, so an environment variable is the
only working path — `cortex login` cannot complete without a browser.

Verify the token before spending a turn:

```bash
cortex whoami
```

A non-zero exit means the credential is missing or rejected. Fail the job there
rather than letting every later step fail on its own.

## 2. GitHub Actions

```yaml
name: cortex-review
on: pull_request

jobs:
  review:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0          # a base branch needs real history

      - name: Install Cortex CLI
        run: curl -fsSL https://software.cortex.foundation/install.sh | sh

      - name: Review the change
        env:
          CORTEX_API_KEY: ${{ secrets.CORTEX_API_KEY }}
        run: |
          cortex run --bare --ephemeral \
            --format json \
            "Review the diff against origin/${{ github.base_ref }}. Report findings only."
```

`fetch-depth: 0` matters: a shallow clone has no merge base, so a
`git diff base...head` review sees the wrong change set.

## 3. GitLab CI

```yaml
cortex-review:
  stage: test
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"
  variables:
    # Masked + protected in Settings → CI/CD → Variables.
    CORTEX_API_KEY: $CORTEX_API_KEY
  script:
    - curl -fsSL https://software.cortex.foundation/install.sh | sh
    - cortex exec --review-only --auto read-only
        --output-format json
        --review-base "$CI_MERGE_REQUEST_TARGET_BRANCH_NAME"
        "Report findings only."
```

Mark the variable **masked** so it never appears in job logs, and **protected**
if only protected branches should be able to read it.

## 4. Any other CI

```bash
set -euo pipefail

export CORTEX_API_KEY="$(cat /run/secrets/cortex_api_key)"
cortex whoami

cortex run --bare --ephemeral \
  --format json \
  --output-file result.json \
  "Summarize what changed in this branch and flag anything risky."

# `--format json` prints one document; `result.json` holds the message text.
```

## 5. Pick the right command

| Need | Command |
|---|---|
| One message, one result, no session file | `cortex run --bare --ephemeral` |
| Structured result for a parser | `cortex run --format json` |
| Event-by-event stream | `cortex run --format jsonl` |
| Read-only review that never writes | `cortex exec --review-only` |
| Autonomy pinned by risk | `cortex exec --auto read-only` |
| Several turns over one pipe | `cortex exec --input-format stream-jsonl -o stream-json` |
| Validate the result shape | `cortex run --format json --json-schema` |

`--bare` drops the terminal chrome (progress lines, spacing, annotations) so
stdout is only the result. `--ephemeral` removes the session rollout file when
the run finishes, so a CI job leaves nothing behind for `cortex sessions`.

## 6. Exit codes

| Code | Meaning |
|---|---|
| `0` | The task completed. |
| non-zero | The task did not complete: the run failed, was interrupted, was truncated, or the service was unreachable. |

A non-zero exit is the contract. Do not wrap a failing run in `|| true`; the
result document also carries `success`, `complete`, `interrupted`, and
`truncated` so a parser can tell the cases apart without reading stderr.

When the coding service cannot be reached, the CLI prints
*The coding service is temporarily unavailable* and exits non-zero. It never
falls back to a local model.

## 7. Read the result

`cortex run --format json` prints one document:

```json
{
  "type": "result",
  "session_id": "…",
  "message": "the final answer",
  "events": 12,
  "success": true,
  "interrupted": false,
  "complete": true,
  "truncated": false,
  "finish_reason": "stop"
}
```

`--json-schema` validates that document against the shipped schema before it is
printed, so a shape change fails the job instead of silently breaking a parser.
Print the schema itself when writing the parser:

```bash
cortex schema list
cortex schema print run-result
cortex schema print exec-result
```

`cortex exec -o json` prints the same idea with its own field names
(`subtype`, `is_error`, `duration_ms`, `num_turns`) and validates against
`exec-result`.

## 8. Multi-turn over one pipe

`cortex exec --input-format stream-jsonl -o stream-json` reads one JSON object
per line and writes one stream back. The connection outlives each turn, so a
caller can follow up without restarting the process.

```bash
printf '%s\n' \
  '{"text":"add a retry helper to the api client"}' \
  '{"text":"now cover the give-up path with a test"}' \
  '{"control":"shutdown"}' \
| CORTEX_API_KEY="$CORTEX_API_KEY" \
  cortex exec --input-format stream-jsonl -o stream-json --auto read-only
```

`{"control":"interrupt"}` stops the running turn without ending the stream. A
line that is not a usable turn produces an error event and the stream
continues, so one bad line does not drop the rest of the input.

## 9. Keep the run contained

- `--auto read-only` is the default and the safest: the sandbox cannot write.
- `--review-only` pins both halves (read-only sandbox, no auto-approved writes)
  and refuses to start when combined with a write-widening flag.
- `--ephemeral` leaves no session file.
- Network egress is blocked by default. A run that needs a host needs it in
  `.cortex/sandbox.toml`; the list is explicit and fails closed.
- `--bare` keeps stdout to the result, so a stray log line cannot corrupt a
  parsed document.

## 10. Troubleshooting

| Symptom | Cause |
|---|---|
| `cortex whoami` fails in CI | The secret is not exported for this step, or the job is on an unprotected branch. |
| Exit non-zero with *The coding service is temporarily unavailable* | The API is unreachable from the runner, or a proxy blocks egress. |
| The review sees no changes | The clone is shallow. Use `fetch-depth: 0`. |
| A parse error on stdout | Something else printed to stdout. Add `--bare` and read stderr for logs. |
| The job leaves sessions behind | Add `--ephemeral`. |

## See also

- [Headless execution](exec.md) — every flag on `run` and `exec`.
- [Environment variables](../configuration/env.md) — the full variable list.
- [Login reference](../reference/login.md) — tokens, `--with-api-key`, and CI notes.
- [CI secrets](../CI_SECRETS.md) — secrets for this repository's own release pipeline.
