// Controlled wire peer. No Cortex executable, credentials, or network.
import { spawn } from "node:child_process";
import { writeFileSync } from "node:fs";
const args = process.argv.slice(2);
let prompt = "";
for await (const chunk of process.stdin) prompt += chunk;
const env = { session_id: args.includes("--session-id") ?
  args[args.indexOf("--session-id") + 1] : "00000000-0000-4000-8000-000000000001", timestamp: 1 };
const emit = (v) => process.stdout.write(JSON.stringify({ ...env, ...v }) + "\n");
if (process.env.CAPTURE) writeFileSync(process.env.CAPTURE, JSON.stringify({ args, prompt }));
if (prompt === "spawn-child") {
  const child = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"],
    { stdio: "inherit" });
  writeFileSync(process.env.CHILD_PID, String(child.pid));
}
if (prompt === "wrong-resume") env.session_id = "00000000-0000-4000-8000-000000000003";
emit({ type: "system", subtype: "init", cwd: "/fixture", model: "cortex" });
if (prompt === "hang" || prompt === "spawn-child") {
  process.on("SIGINT", () => {});
  setInterval(() => {}, 1000);
} else if (prompt === "malformed") {
  process.stdout.write("not-json\n");
} else if (prompt === "oversized") {
  process.stdout.write("x".repeat(8192));
} else if (prompt === "stderr") {
  process.stderr.write("private-fixture-data".repeat(1024));
} else if (prompt === "service-error") {
  emit({ type: "error", message: "private-fixture-data" });
} else if (prompt !== "missing-completion") {
  const line = Buffer.from(JSON.stringify({ ...env, type: "delta", content: "héllo" }) + "\r\n");
  for (const byte of line) process.stdout.write(Buffer.from([byte]));
  emit({ type: "completion", finalText: prompt, numTurns: 1, durationMs: 5, toolCalls: 0,
    ...(prompt === "legacy-completion" ? {} : { success: prompt !== "failed-completion", error: null }) });
  if (prompt === "late-error") process.exitCode = 7;
  if (prompt === "extra-event") emit({ type: "delta", content: "late" });
  if (prompt === "hang-after-completion") setInterval(() => {}, 1000);
}
