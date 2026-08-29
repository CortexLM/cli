# Product-facing errors

Users see Cortex, not vendors.

- When Chat/Code, tools, computers, plugins, or snapshots cannot reach the Cortex API, show: **The coding service is temporarily unavailable**.
- Never surface raw provider, SDK, crate, or HTTP-client names (`reqwest`, `hyper`, OpenAI, Anthropic, Grok, …) in TUI, CLI, or user logs.
- Auth failures: tell the user to run `cortex login` or set `CORTEX_API_KEY`. Do not dump token bodies or WorkOS internals.
- Rate / billing errors may mention `cortex.foundation` billing pages. Do not mention a third-party processor by name.
- `Display` / `user_friendly_message()` on `CortexError` is the source of truth. TUI and CLI must use that, not `format!("{e:?}")`.
- Tests must assert the product string, not a mock "success" when the API is down.
- Internal traces (`tracing::error!`) may keep structured fields for operators; they must not be copied into the transcript the user reads.
