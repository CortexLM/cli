import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createHash } from "node:crypto";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { createInterface } from "node:readline";
import { checkNode } from "../host.mjs";
import { build } from "../build.mjs";

test("explicit Node 22 minimum and no unsupported TypeScript transforms", async () => {
  assert.throws(() => checkNode("20.19.0"));
  assert.throws(() => checkNode("22.12.0"));
  checkNode("22.13.0");
  const root = await mkdtemp(join(tmpdir(), "cortex-host-"));
  try {
    await writeFile(join(root, "bad.ts"), "enum NotErasable { A, B }");
    assert.throws(() => build(join(root, "bad.ts"), join(root, "out.mjs")));
    await writeFile(join(root, "good.ts"), "const value: number = 42; export default value;");
    build(join(root, "good.ts"), join(root, "out.mjs"));
    assert.match(await readFile(join(root, "out.mjs"), "utf8"), /42/);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test("real versioned host handshake, arguments, context and persistent state", async () => {
  const root = await mkdtemp(join(tmpdir(), "cortex-host-"));
  let child;
  try {
    const path = join(root, "plugin.mjs");
    await writeFile(path, `let n=0; export default {protocol:1,
      init:()=>({data:null}),shutdown:()=>({data:n}),
      commands:{hello:(args,context)=>({data:{args,context,n:++n}})}};`);
    const host = await readFile(new URL("../host.mjs", import.meta.url), "utf8");
    child = spawn(process.execPath, ["--input-type=module", "--eval", host, "--", "--cortex-plugin-host"], { env: {}, stdio: ["pipe", "pipe", "pipe"] });
    const lines = createInterface({ input: child.stdout })[Symbol.asyncIterator]();
    let id = 0;
    async function request(method, extra = {}) {
      child.stdin.write(JSON.stringify({ protocol: 1, id: ++id, method, ...extra }) + "\n");
      const reply = JSON.parse((await lines.next()).value);
      assert.equal(reply.id, id);
      assert.equal(reply.protocol, 1);
      return reply;
    }
    const hash = createHash("sha256").update(await readFile(path)).digest("hex");
    const handshake = await request("handshake", { entrypoint: path, sha256: hash, manifest: {
      id: "fixture", version: "0.1.0", commands: [{name:"hello"}], hooks: [], tools: [],
    } });
    assert.equal(handshake.result.sha256, hash);
    await request("init", {context:{}});
    assert.deepEqual((await request("command", {name:"hello",input:["Ada"],context:{call_id:"42"}})).result.data,
      {args:["Ada"],context:{call_id:"42"},n:1});
    assert.equal((await request("command", {name:"hello",input:[],context:{}})).result.data.n, 2);
    assert.equal((await request("command", {name:"missing",input:[],context:{}})).error, "Plugin request failed");
    assert.equal((await request("shutdown", {context:{}})).result.data, 2);
    child.stdin.end();
    assert.equal((await once(child, "exit"))[0], 0);
  } finally {
    child?.kill();
    await rm(root, { recursive: true, force: true });
  }
});
