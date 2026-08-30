# API and product

- Default API origin: `https://api.cortex.foundation`.
- Device login: `POST /v1/auth/device` and `POST /v1/auth/device/token` on the API origin. Do not call `/v1/auth/device/code` (404). `auth.cortex.foundation` is not the device endpoint.
- Software / updates: `https://software.cortex.foundation`.
- The harness is a client. Chat/Code turns, tools, computers, plugins, and snapshots go through that API. Do not embed a second model provider.
- Code turns: `POST /v1/code/sessions/{id}/turns` with `{ message, mode: "chat"|"code" }`. Plan/ask are TUI/harness locks, not API modes.
- When the API is unreachable, the product fails closed with a product-facing error. Do not fall back to a local mock model.
- Environment overrides (`CORTEX_API_URL`, `CORTEX_API_KEY`, `CORTEX_AUTH_TOKEN`) are for operators and tests. Defaults must stay production Cortex URLs.
