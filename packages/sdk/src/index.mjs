import { isAbsolute } from "node:path";
import { CortexError } from "./errors.mjs";
import { launch } from "./process.mjs";
import { validSessionId } from "./protocol.mjs";

export { CortexError } from "./errors.mjs";
export { PROTOCOL } from "./protocol.mjs";

const defaults = Object.freeze({
  maxPromptBytes: 256 * 1024,
  maxLineBytes: 1024 * 1024,
  maxOutputBytes: 16 * 1024 * 1024,
  maxStderrBytes: 1024 * 1024,
  maxEvents: 100_000,
  killGraceMs: 500,
});
const safeText = (v) => typeof v === "string" && !v.includes("\0");
const positive = (v) => Number.isSafeInteger(v) && v > 0 && v <= 2_147_483_647;

export class CortexClient {
  #options;
  constructor(options) {
    if (!options || !safeText(options.executable) || !isAbsolute(options.executable) ||
        !safeText(options.cwd) || !isAbsolute(options.cwd)) {
      throw new CortexError("invalid_options");
    }
    const resolved = { ...defaults, ...options, prefixArgs: options.prefixArgs ?? [] };
    if (!Array.isArray(resolved.prefixArgs) || !resolved.prefixArgs.every(safeText) ||
        Object.keys(defaults).some((key) => !positive(resolved[key]))) {
      throw new CortexError("invalid_options");
    }
    // Copy so later caller mutations cannot swap arguments or the environment.
    this.#options = { ...resolved, prefixArgs: [...resolved.prefixArgs],
      env: options.env === undefined ? undefined : { ...options.env } };
  }

  start(prompt, options = {}) { return this.#run(prompt, options); }

  resume(sessionId, prompt, options = {}) {
    if (!validSessionId(sessionId)) throw new CortexError("invalid_options");
    return this.#run(prompt, options, sessionId.toLowerCase());
  }

  #run(prompt, options, sessionId) {
    const timeoutMs = options.timeoutMs ?? 600_000;
    if (!safeText(prompt) || !prompt.trim() ||
        Buffer.byteLength(prompt) > this.#options.maxPromptBytes || !positive(timeoutMs) ||
        (options.onEvent !== undefined && typeof options.onEvent !== "function") ||
        (options.signal !== undefined && !(options.signal instanceof AbortSignal))) {
      throw new CortexError("invalid_options");
    }
    // No prompt, credential, shell string, schema, or unimplemented model flag
    // is put on argv. This client deliberately offers read-only turns only.
    const args = ["exec", "--output-format", "stream-json", "--auto", "read-only",
      "--cwd", this.#options.cwd, "--timeout", String(Math.ceil(timeoutMs / 1000))];
    if (sessionId) args.push("--session-id", sessionId);
    return launch(this.#options, args, prompt, { ...options, timeoutMs }, sessionId);
  }
}
