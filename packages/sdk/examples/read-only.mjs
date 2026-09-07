import { CortexClient, CortexError } from "../src/index.mjs";
import { resolve } from "node:path";

// Run only intentionally, after configuring Cortex login outside this program:
// node packages/sdk/examples/read-only.mjs /absolute/path/to/Cortex /workspace
const client = new CortexClient({
  executable: resolve(process.argv[2]),
  cwd: resolve(process.argv[3]),
});
const controller = new AbortController();
process.once("SIGINT", () => controller.abort());
try {
  const first = await client.start("Explain the project structure without modifying files.", {
    signal: controller.signal,
  }).result;
  console.log(first.text);
  console.log(`Session: ${first.sessionId}`);
  // Explicit continuation: client.resume(first.sessionId, "Explain the test layout.")
} catch (error) {
  console.error(error instanceof CortexError ? error.message : "Cortex execution failed");
  process.exitCode = 1;
}
