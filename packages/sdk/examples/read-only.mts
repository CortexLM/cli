import { CortexClient, CortexError } from "../src/index.mjs";
import type { CortexEvent, TurnResult } from "../src/index.mjs";

export async function analyze(
  executable: string,
  workspace: string,
  onEvent: (event: CortexEvent) => void,
  signal?: AbortSignal,
): Promise<TurnResult> {
  const client = new CortexClient({ executable, cwd: workspace });
  try {
    return await client.start("Explain the test layout without modifying files.", {
      onEvent, signal, timeoutMs: 60_000,
    }).result;
  } catch (error) {
    if (error instanceof CortexError && error.code === "cancelled") {
      // The local subprocess is stopped; this is not a remote cancel receipt.
    }
    throw error;
  }
}
