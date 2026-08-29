# API and product

- Default API origin: `https://api.cortex.foundation`.
- Auth origin: `https://auth.cortex.foundation` (WorkOS session / device code / existing `/v1` auth).
- Software / updates: `https://software.cortex.foundation`.
- The harness is a client. Chat/Code turns, tools, computers, plugins, and snapshots go through that API. Do not embed a second model provider.
- Keep `/v1/models`, `/v1/responses` (or the current Chat/Code route in `cortex-engine`), and `/v1` auth paths. If the backend adds a route, update the client and the user-facing error map together.
- When the API is unreachable, the product fails closed with a product-facing error. Do not fall back to a local mock model.
- Environment overrides (`CORTEX_API_URL`, `CORTEX_API_KEY`, `CORTEX_AUTH_TOKEN`) are for operators and tests. Defaults must stay production Cortex URLs.
