import { CortexError } from "./errors.mjs";

export const PROTOCOL = "cortex.exec.stream-json/0.1.8";

const string = (v) => typeof v === "string";
const number = (v) => Number.isSafeInteger(v) && v >= 0;
const object = (v) => v !== null && typeof v === "object" && !Array.isArray(v);
const session = (v) => string(v) && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(v);

const shapes = {
  system: (e) => e.subtype === "init" && string(e.cwd) && string(e.model),
  delta: (e) => string(e.content),
  message: (e) => e.role === "assistant" && string(e.id) && string(e.text),
  reasoning: (e) => string(e.text),
  tool_call: (e) => string(e.id) && string(e.toolName) &&
    (e.parameters === null || object(e.parameters)),
  tool_result: (e) => string(e.id) && string(e.toolName) &&
    typeof e.isError === "boolean" && "value" in e && number(e.durationMs),
  error: (e) => string(e.message),
  completion: (e) => string(e.finalText) && number(e.numTurns) &&
    number(e.durationMs) && number(e.toolCalls) && typeof e.success === "boolean" &&
    (e.error === undefined || e.error === null || (!e.success && string(e.error))),
};

export function validSessionId(value) { return session(value); }

/** Bounded byte framing, including split UTF-8 characters and CRLF. */
export class EventStream {
  #pending = [];
  #pendingBytes = 0;
  #total = 0;
  #count = 0;
  #session;
  #completed = false;

  constructor(limits, requestedSession, onEvent) {
    this.limits = limits;
    this.requestedSession = requestedSession;
    this.onEvent = onEvent;
    this.completion = undefined;
  }

  push(chunk) {
    this.#total += chunk.length;
    if (this.#total > this.limits.maxOutputBytes) throw new CortexError("output_limit");
    let from = 0;
    for (let end = chunk.indexOf(10); end !== -1; end = chunk.indexOf(10, from)) {
      this.#append(chunk.subarray(from, end));
      this.#line(Buffer.concat(this.#pending, this.#pendingBytes));
      this.#pending = [];
      this.#pendingBytes = 0;
      from = end + 1;
    }
    this.#append(chunk.subarray(from));
  }

  #append(bytes) {
    if (this.#pendingBytes + bytes.length > this.limits.maxLineBytes) {
      throw new CortexError("output_limit");
    }
    if (bytes.length) {
      this.#pending.push(bytes);
      this.#pendingBytes += bytes.length;
    }
  }

  #line(bytes) {
    if (++this.#count > this.limits.maxEvents) throw new CortexError("output_limit");
    let event;
    try {
      event = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
    } catch {
      throw new CortexError("protocol_error");
    }
    if (!object(event) || !Object.hasOwn(shapes, event.type) ||
        !shapes[event.type](event) || !session(event.session_id) ||
        !number(event.timestamp) || this.#completed ||
        (event.server !== undefined && !string(event.server)) ||
        (event.exitCode !== undefined && !Number.isSafeInteger(event.exitCode))) {
      throw new CortexError("protocol_error");
    }
    if (!this.#session) {
      if (event.type !== "system" ||
          (this.requestedSession && event.session_id !== this.requestedSession)) {
        throw new CortexError("protocol_error");
      }
      this.#session = event.session_id;
    } else if (event.type === "system" || event.session_id !== this.#session) {
      throw new CortexError("protocol_error");
    }
    // Never copy raw service errors (or stderr) into public exceptions/events.
    if (event.type === "error" || (event.type === "completion" && !event.success)) {
      throw new CortexError("execution_failed");
    }
    if (event.type === "completion") {
      this.#completed = true;
      this.completion = structuredClone(event);
    }
    if (this.onEvent) {
      try {
        const returned = this.onEvent(event);
        if (returned?.then) {
          Promise.resolve(returned).catch(() => {});
          throw new Error("Observers must be synchronous");
        }
      } catch {
        throw new CortexError("observer_failed");
      }
    }
  }

  finish() {
    // JSONL requires newline termination; EOF is not a completion event.
    if (this.#pendingBytes || !this.#completed) throw new CortexError("protocol_error");
    return this.completion;
  }
}
