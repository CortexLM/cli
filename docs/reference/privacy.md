# Privacy and diagnostic data

The coding product sends user-approved prompts/code/tool results to the Cortex
coding service as described by the product. The local diagnostics added here
do **not** change that business traffic and do not add diagnostic destinations.

| Data | Location | Handling |
| --- | --- | --- |
| Authentication | OS keyring or protected process environment | Never log, snapshot, commit, or attach |
| Sessions, messages, tool outputs | Existing session stores | May contain personal/customer data; minimize access and retention |
| Local diagnostic events | Explicit `CORTEX_DIAGNOSTICS_DIR` | Closed allowlist, no content or user IDs, bounded seven-day retention |
| CI test/coverage reports | Repository CI artifacts | Synthetic test data only; 14–30 day retention |
| CPU profiles / existing debug logs | Operator-selected local files | Potentially sensitive; no automatic upload |

Consent is explicit: diagnostics are disabled unless the operator chooses an
output directory. Unset the variable to stop recording. Remove only that
operator-selected generated diagnostic directory to erase diagnostic history.
This does not erase session stores, backups, keyring credentials, or third-party
service data. Handle those separately through their existing lifecycle.

For bug reports, use aggregate counts and a minimal synthetic reproduction.
Review any attachment manually. Existing `--debug` logs and session exports
are not made safe by this diagnostic allowlist. Do not feed them to external
analytics or automated issue creation.

There is no claim of regulatory certification, centralized consent management,
or remote deletion guarantees in this repository. Retention/consent obligations
for deployment operators remain their responsibility.
