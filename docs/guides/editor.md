# Editor integration boundary

There is no packaged or marketplace-tested editor extension in this change.
The [local client SDK](../reference/sdk.md) is an integration building block;
running Cortex in an editor terminal does not establish VS Code, JetBrains,
Zed, ACP-client, or editor-fork compatibility.

An editor adapter can explicitly launch `CortexClient.start` and render text/tool
observations, retain the returned session ID, call `resume` for a user-requested
follow-up, and await `cancel` when the user cancels or closes its panel. The SDK
does not expose write approvals or arbitrary execution-policy settings.

Before implementing or enabling a named editor adapter:

1. Require workspace trust and explicit user initiation before starting Cortex.
   Do not run from activation, workspace settings, opening a file, or background
   indexing.
2. Use an operator-configured **absolute** executable path, not a repository
   script, shell command string, downloaded binary, or workspace-supplied
   `prefixArgs`.
3. Let the user select the workspace root. Send only explicitly selected context;
   do not silently read all tabs, unsaved documents, credentials, or other roots.
4. Explain that prompts/context go through the installed CLI to the coding
   service. Do not log them, stderr, tokens, or full tool results.
5. Render streaming text as untrusted data, not executable markup. Treat
   completion events as provisional until the result promise resolves.
6. Bind cancellation to editor disposal and wait for local process cleanup.
   Do not label local disconnect as confirmed remote cancellation.
7. Test trust denial, selection bounds, executable/argv handling, service failure,
   resume identity and cancellation using injected adapters and real fixture
   subprocesses before testing an actual editor host.

The SDK's typed example is
`packages/sdk/examples/read-only.mts`; its JavaScript example must only be run
intentionally after configuring Cortex login. Neither example installs an
extension or contacts a service as part of the offline test suite.

## Other automation

GitHub analysis and its explicit publication boundary are documented in the
[SDK reference](../reference/sdk.md#github-automation-boundary).
PR suggestion application (`Cortex pr --apply`) remains unsupported and fails
before repository/account access rather than reporting that changes were applied.

Commandless DAG tasks now fail explicitly because no DAG agent was started.
Command tasks use the normal engine tool router, request read-only authority,
and do not obtain approval from task metadata. Commands needing more authority
must be run through the ordinary approved CLI flow, not smuggled through a DAG.
Failures/timeouts are counted, failed dependents are skipped, fail-fast stops
new admission, and running work is drained through its normal owner.
Persisted interrupted-running tasks require reconciliation before resume;
failed completed DAGs do not become successful by resuming them.

Slack transport source is not an installed coding bot. Shipping one still
requires a parent application that wires an authenticated session handler,
workspace/user authorization, thread/session continuity, deduplication, explicit
publication policy, and OAuth/token lifecycle. No Slack workspace, bot account,
or handler was configured or exercised for this change. Placeholder transport
code is not evidence of an operational first-party Slack integration.
