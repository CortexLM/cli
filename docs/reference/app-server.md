# Local app-server API

This is the standalone `cortex-server`, not the remote Cortex coding API.
Start it from the workspace whose files it may access:

```bash
cargo build --locked -p cortex-app-server
./target/debug/cortex-server --listen 127.0.0.1:55554 --json-logs --auth
```

Supply `CORTEX_SERVER_API_KEY` from a local secret manager or protected process
environment. `CORTEX_JWT_SECRET` enables JWT verification instead. For compatibility,
`CORTEX_API_KEY` is still accepted by environment-only server configuration;
prefer the distinct server key so coding-service and local-server credentials
are not reused. Never put values in checked-in JSON, examples, shell history,
or command arguments.

`--config FILE` reads a JSON `ServerConfig`. Environment server credentials are
applied to both file and environment configuration. `--listen` explicitly
overrides either. `--auth` requires configured credentials.
The CLI's `Cortex serve` command also accepts `CORTEX_SERVER_API_KEY` and
`CORTEX_JWT_SECRET`; prefer these to its legacy command-line token option.

The default listener is loopback. A non-loopback listener without authentication
fails before binding. Use a trusted TLS reverse proxy for remote access; direct
TLS is not implemented and a nonempty TLS configuration fails explicitly.
mDNS is opt-in. Empty `cors_origins` denies cross-origin browser access; list
the exact trusted browser origins in configuration.

## Contract

[OpenAPI 3.1 JSON](app-server.openapi.json) is generated from the handler models
with schemars 0.8.22 and served at authenticated `GET /api/v1/openapi.json`.
The supported contract covers:

| Method | Path (under `/api/v1`) | Meaning |
| --- | --- | --- |
| GET | `/health` | Anonymous local readiness, **not** remote coding-service health |
| GET | `/metrics` | Authenticated request/error/session counters |
| POST, GET | `/sessions` | Create/list in-memory sessions |
| GET, DELETE | `/sessions/{id}` | Read/delete an in-memory session |
| POST, GET | `/sessions/{id}/messages` | Store/list messages, no model inference |

Other development endpoints, including files, terminals, admin, SSE, and
WebSockets, are not yet part of this stable schema. Authentication applies to
them too. A configured server API key is an operator credential, not a
multi-tenant sandbox. JWT admin routes require the `admin` role; ordinary
authenticated routes operate on the server's workspace. Do not host mutually
untrusted tenants in one process.

Send `Authorization: ApiKey <server key>` or `Authorization: Bearer <JWT>`.
JWTs require issuer `Cortex` and audience `cortex-api`.
WebSockets require authentication at the HTTP upgrade; query-string tokens do
not grant access. Client `Auth` messages verify JWTs rather than accepting any
string. Anonymous endpoint exceptions match exact paths, never prefixes.

Every response has `X-Request-Id`, `traceparent`, and `X-Response-Time`.
Only UUID request IDs and valid W3C trace contexts are retained from callers.
These support local correlation. No exporter sends diagnostic data elsewhere.
The timing covers response creation/headers, not the lifetime of an SSE stream.

Rate limiting uses the connected peer address unless explicitly configured to
trust proxy headers. Configure that only behind a trusted proxy. Limits also
apply to bodies without `Content-Length`; a limit failure returns 413.
API file operations are constrained to the opened workspace, not every home,
temporary directory, or mounted drive.

## Maintain generated contracts

```bash
python3 scripts/readiness/schema.py --write
python3 scripts/readiness/schema.py
```

The [CLI command inventory](cli.commands.json) is generated from Clap 4.6.6 on Linux.
The remote coding API contract is maintained by its service; the client-side
contract and tested status mappings are in
[`code_agent.rs`](../../src/cortex-engine/src/client/code_agent.rs).
Do not invent undocumented remote endpoints.
