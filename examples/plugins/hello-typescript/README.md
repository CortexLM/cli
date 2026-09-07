# Hello TypeScript

A real Cortex protocol-1 Node plugin: persistent call counter, command, schema
validated tool, before-tool argument mutation/veto, and lifecycle notification.

```sh
Cortex plugin build --path examples/plugins/hello-typescript
Cortex plugin validate --path examples/plugins/hello-typescript --json
Cortex plugin install ./examples/plugins/hello-typescript --trust-code
Cortex plugin run hello-typescript hello Ada --json
Cortex plugin run hello-typescript --tool greet --input '{"name":"  Ada  "}' --json
```

The `blocked` name exits nonzero without calling the tool.
`Cortex plugin disable hello-typescript` persists across restarts.

Requires Node 22.13+ in the 22 LTS line. No npm install is needed. Type stripping
is not type checking. Trust permits native code execution, **not a sandbox**;
only install code you explicitly trust. See the
[runtime contract and integration limits](../../../packages/plugin-host/README.md).
