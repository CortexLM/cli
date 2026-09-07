import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { CortexClient, CortexError, PROTOCOL } from "../src/index.mjs";

const fixture = fileURLToPath(new URL("./fixture.mjs", import.meta.url));
const client = (options = {}) => new CortexClient({
  executable: process.execPath, prefixArgs: [fixture], cwd: tmpdir(), env: {},
  killGraceMs: 30, ...options,
});
const failure = (code) => (e) => e instanceof CortexError && e.code === code &&
  !e.message.includes("private-fixture-data");

test("real subprocess start, UTF-8 framing, argv boundaries, and stdin privacy", async () => {
  const dir = await mkdtemp(join(tmpdir(), "cortex-sdk-"));
  try {
    const capture = join(dir, "capture.json");
    const events = [];
    const prompt = '--help ; " literal prompt\nsecond line';
    const turn = client({ env: { CAPTURE: capture }, cwd: dir }).start(prompt, {
      onEvent: (e) => { events.push(e); },
    });
    const result = await turn.result;
    assert.equal(result.text, prompt);
    assert.equal(turn.sessionId, "00000000-0000-4000-8000-000000000001");
    assert.deepEqual(events.map((e) => e.type), ["system", "delta", "completion"]);
    assert.equal(events[1].content, "héllo");
    const request = JSON.parse(await readFile(capture, "utf8"));
    assert.equal(request.prompt, prompt);
    assert.deepEqual(request.args, ["exec", "--output-format", "stream-json",
      "--auto", "read-only", "--cwd", dir, "--timeout", "600"]);
  } finally { await rm(dir, { recursive: true, force: true }); }
});

test("resume preserves the requested session or fails closed", async () => {
  const result = await client().resume("00000000-0000-4000-8000-000000000002", "continue").result;
  assert.equal(result.sessionId, "00000000-0000-4000-8000-000000000002");
  await assert.rejects(client().resume("00000000-0000-4000-8000-000000000002", "wrong-resume").result,
    failure("protocol_error"));
  assert.throws(() => client().resume("../session", "continue"), failure("invalid_options"));
});

for (const [prompt, code] of [
  ["malformed", "protocol_error"], ["missing-completion", "protocol_error"],
  ["extra-event", "protocol_error"], ["legacy-completion", "protocol_error"],
  ["failed-completion", "execution_failed"], ["late-error", "execution_failed"],
  ["service-error", "execution_failed"], ["oversized", "output_limit"],
  ["stderr", "output_limit"], ["hang-after-completion", "timeout"],
]) {
  test(`rejects ${prompt} without false success or private errors`, async () => {
    await assert.rejects(client({ maxLineBytes: 2048, maxStderrBytes: 2048 })
      .start(prompt, { timeoutMs: 500 }).result, failure(code));
  });
}

test("spawn failure is sanitized", async () => {
  await assert.rejects(client({ executable: resolve(tmpdir(), "missing-cortex-executable") })
    .start("hello").result, failure("spawn_failed"));
});
test("timeout and explicit cancellation clean up a stubborn child", async () => {
  await assert.rejects(client().start("hang", { timeoutMs: 150 }).result, failure("timeout"));
  const turn = client().start("hang");
  await turn.cancel();
  await assert.rejects(turn.result, failure("cancelled"));
});
test("pre-abort never spawns and observer exceptions kill the process", async () => {
  await assert.rejects(client({ executable: "/does-not-exist" })
    .start("hello", { signal: AbortSignal.abort() }).result, failure("cancelled"));
  await assert.rejects(client().start("hang", {
    onEvent() { throw new Error("private-fixture-data"); },
  }).result, failure("observer_failed"));
});
test("invalid option limits and nonabsolute executables are rejected", () => {
  assert.throws(() => client({ executable: "Cortex" }), failure("invalid_options"));
  assert.throws(() => client({ maxEvents: 0 }), failure("invalid_options"));
  assert.throws(() => client().start(""), failure("invalid_options"));
});
test("cancellation terminates same-group descendants on Linux", {
  skip: process.platform !== "linux",
}, async () => {
  const dir = await mkdtemp(join(tmpdir(), "cortex-sdk-child-"));
  try {
    const path = join(dir, "pid");
    let ready;
    const initialized = new Promise((resolve) => { ready = resolve; });
    const turn = client({ env: { CHILD_PID: path } }).start("spawn-child", {
      onEvent: () => { ready(); },
    });
    await initialized;
    const pid = Number(await readFile(path, "utf8"));
    await turn.cancel();
    await assert.rejects(turn.result, failure("cancelled"));
    // Orphaned children may briefly be zombies waiting for container PID 1.
    let alive = true;
    for (let i = 0; i < 50 && alive; i++) {
      try { alive = !(await readFile(`/proc/${pid}/stat`, "utf8")).includes(") Z "); }
      catch { alive = false; }
      if (alive) await new Promise((r) => setTimeout(r, 20));
    }
    assert.equal(alive, false);
  } finally { await rm(dir, { recursive: true, force: true }); }
});
test("package version follows VERSION_CLI and Apache license is included", async () => {
  const metadata = JSON.parse(await readFile(new URL("../package.json", import.meta.url)));
  const version = (await readFile(new URL("../../../VERSION_CLI", import.meta.url), "utf8")).trim();
  assert.equal(metadata.version, version);
  assert.equal(metadata.license, "Apache-2.0");
  assert.equal(metadata.private, true);
  assert.equal(PROTOCOL, "cortex.exec.stream-json/0.1.8");
  assert.ok((await stat(new URL("../LICENSE", import.meta.url))).isFile());
});


test("event and total-byte bounds fail without buffering unbounded output", async () => {
  await assert.rejects(client({ maxEvents: 1 }).start("hello").result, failure("output_limit"));
  await assert.rejects(client({ maxOutputBytes: 100 }).start("hello").result, failure("output_limit"));
});
test("asynchronous observers are rejected instead of leaving unhandled failures", async () => {
  await assert.rejects(client().start("hello", {
    async onEvent() { throw new Error("private-fixture-data"); },
  }).result, failure("observer_failed"));
});
