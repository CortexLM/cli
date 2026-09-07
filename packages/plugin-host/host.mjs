// Cortex plugin-host protocol 1. Embedded into the CLI; no package installation.
import { pathToFileURL } from "node:url";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";

export const protocol = 1;
export const maxFrameBytes = 1024 * 1024;

export function checkNode(version = process.versions.node) {
  const [major, minor] = version.split(".").map(Number);
  if (major !== 22 || minor < 13) throw new Error("Node 22.13 or later in the Node 22 LTS line is required");
}

// Tests import this file; the Rust supervisor sets this explicit launch switch.
if (process.argv.includes("--cortex-plugin-host")) {
  let plugin;
  let initialized = false;
  let manifest;
  let buffer = Buffer.alloc(0);
  let chain = Promise.resolve();
  const send = process.stdout.write.bind(process.stdout);
  // Plugin console output is not protocol data, nor an unbounded transcript.
  for (const name of ["log", "info", "warn", "error", "debug"]) console[name] = () => {};

  function reply(id, result, error) {
    const frame = JSON.stringify({ protocol, id, ...(error ? { error } : { result }) });
    if (Buffer.byteLength(frame) > maxFrameBytes) {
      send(JSON.stringify({ protocol, id, error: "Plugin response exceeds the frame limit" }) + "\n");
      return;
    }
    send(frame + "\n");
  }

  function registered(kind, name) {
    if (kind === "command") return manifest.commands.some(c => c.name === name);
    if (kind === "tool") return manifest.tools.some(t => t.name === name);
    return manifest.hooks.some(h => h.function === name);
  }

  async function handle(request) {
    if (request.protocol !== protocol || !Number.isSafeInteger(request.id)) {
      throw new Error("Unsupported protocol or invalid request ID");
    }
    if (request.method === "handshake") {
      if (plugin) throw new Error("Duplicate handshake");
      checkNode();
      manifest = request.manifest;
      const bytes = await readFile(request.entrypoint);
      if (createHash("sha256").update(bytes).digest("hex") !== request.sha256) {
        throw new Error("Plugin artifact changed before import");
      }
      // This executes explicitly trusted native code. This process is NOT a sandbox.
      plugin = (await import(pathToFileURL(request.entrypoint).href)).default;
      if (!plugin || plugin.protocol !== protocol || typeof plugin.init !== "function"
          || typeof plugin.shutdown !== "function") throw new Error("Missing plugin lifecycle exports");
      for (const kind of ["commands", "tools", "hooks"]) {
        const expected = manifest[kind].map(item => kind === "hooks" ? item.function : item.name);
        const actual = Object.keys(plugin[kind] ?? {});
        if (actual.some(name => !expected.includes(name))
            || expected.some(name => typeof plugin[kind]?.[name] !== "function")) {
          throw new Error(`Plugin ${kind} exports disagree with the manifest`);
        }
      }
      return { protocol, id: manifest.id, version: manifest.version, sha256: request.sha256 };
    }
    if (!plugin) throw new Error("Handshake required");
    if (request.method === "init") {
      if (initialized) throw new Error("Plugin is already initialized");
      const result = await plugin.init(request.context);
      initialized = true;
      return result ?? { data: null };
    }
    if (!initialized) throw new Error("Plugin is not initialized");
    if (request.method === "shutdown") {
      initialized = false;
      return (await plugin.shutdown(request.context)) ?? { data: null };
    }
    if (!["command", "tool", "hook"].includes(request.method)
        || !registered(request.method, request.name)) throw new Error("Unregistered plugin operation");
    const group = request.method === "command" ? "commands" : request.method === "tool" ? "tools" : "hooks";
    return await plugin[group][request.name](request.input, request.context);
  }

  function enqueue(line) {
    chain = chain.then(async () => {
      let request;
      try {
        request = JSON.parse(line);
        reply(request.id, await handle(request));
      } catch {
        // Do not pass arbitrary plugin exception content into user-visible logs.
        reply(request?.id ?? null, null, "Plugin request failed");
      }
    });
  }

  process.stdin.on("data", chunk => {
    buffer = Buffer.concat([buffer, chunk]);
    let newline;
    while ((newline = buffer.indexOf(10)) >= 0) {
      if (newline > maxFrameBytes) process.exit(70);
      enqueue(buffer.subarray(0, newline).toString("utf8"));
      buffer = buffer.subarray(newline + 1);
    }
    if (buffer.length > maxFrameBytes) process.exit(70);
  });
  process.stdin.on("end", () => { chain.finally(() => process.exit(0)); });
}
