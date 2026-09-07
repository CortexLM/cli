import { spawn } from "node:child_process";
import { CortexError } from "./errors.mjs";
import { EventStream } from "./protocol.mjs";

/** Own the child until close; never resolve on an early completion or exit. */
export function launch(options, args, prompt, turnOptions, requestedSession) {
  let child;
  let failure;
  let settled = false;
  let killTimer;
  let deadline;
  let drainTimer;
  let sessionId;
  const notify = (event) => {
    if (event.type === "system") sessionId = event.session_id;
    return turnOptions.onEvent?.(event);
  };
  const stream = new EventStream(options, requestedSession, notify);
  const signalGroup = (signal) => {
    if (!child?.pid) return;
    try {
      if (process.platform === "win32") child.kill(signal);
      else process.kill(-child.pid, signal);
    } catch { /* Already exited. */ }
  };
  const stop = (code) => {
    if (settled || failure) return;
    failure = new CortexError(code);
    signalGroup("SIGINT");
    killTimer = setTimeout(() => signalGroup("SIGKILL"), options.killGraceMs);
    // Even a descendant holding a pipe open must not hang the caller forever.
    drainTimer = setTimeout(() => {
      signalGroup("SIGKILL");
      child?.stdin.destroy();
      child?.stdout.destroy();
      child?.stderr.destroy();
    }, options.killGraceMs + 500);
  };
  const abort = () => stop("cancelled");
  const cleanup = () => {
    settled = true;
    clearTimeout(killTimer);
    clearTimeout(deadline);
    clearTimeout(drainTimer);
    turnOptions.signal?.removeEventListener("abort", abort);
    // Terminate same-group descendants, including after a successful parent exit.
    signalGroup("SIGKILL");
  };

  const result = new Promise((resolve, reject) => {
    if (turnOptions.signal?.aborted) {
      settled = true;
      reject(new CortexError("cancelled"));
      return;
    }
    try {
      child = spawn(options.executable, [...options.prefixArgs, ...args], {
        cwd: options.cwd,
        env: options.env,
        shell: false,
        detached: process.platform !== "win32",
        windowsHide: true,
        stdio: ["pipe", "pipe", "pipe"],
      });
    } catch {
      settled = true;
      reject(new CortexError("spawn_failed"));
      return;
    }
    deadline = setTimeout(() => stop("timeout"), turnOptions.timeoutMs);
    turnOptions.signal?.addEventListener("abort", abort, { once: true });
    child.on("error", () => stop("spawn_failed"));
    child.stdin.on("error", () => stop("execution_failed"));
    child.stdout.on("error", () => stop("protocol_error"));
    child.stderr.on("error", () => stop("protocol_error"));
    let stderrBytes = 0;
    child.stderr.on("data", (bytes) => {
      stderrBytes += bytes.length;
      if (stderrBytes > options.maxStderrBytes) stop("output_limit");
    });
    child.stdout.on("data", (chunk) => {
      if (failure) return;
      try { stream.push(chunk); }
      catch (error) { stop(error instanceof CortexError ? error.code : "protocol_error"); }
    });
    child.once("close", (code) => {
      cleanup();
      if (failure) { reject(failure); return; }
      if (code !== 0) { reject(new CortexError("execution_failed", code)); return; }
      try {
        const completion = stream.finish();
        resolve(Object.freeze({
          sessionId: completion.session_id,
          text: completion.finalText,
          numTurns: completion.numTurns,
          durationMs: completion.durationMs,
          toolCalls: completion.toolCalls,
        }));
      } catch (error) { reject(error); }
    });
    child.stdin.end(prompt, "utf8");
  });
  // A user may subscribe to events before awaiting result. Avoid unhandled
  // rejections without changing the promise that they eventually observe.
  result.catch(() => {});
  return Object.freeze({
    result,
    get sessionId() { return sessionId; },
    async cancel() {
      stop("cancelled");
      await result.catch(() => {});
    },
  });
}
