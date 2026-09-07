export declare const PROTOCOL: "cortex.exec.stream-json/0.1.8";
export type ErrorCode = "invalid_options" | "spawn_failed" | "protocol_error" |
  "output_limit" | "execution_failed" | "cancelled" | "timeout" | "observer_failed";
export declare class CortexError extends Error {
  readonly code: ErrorCode;
  readonly exitCode?: number | null;
  constructor(code: ErrorCode, exitCode?: number | null);
}
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
interface Envelope { session_id: string; timestamp: number }
export type CortexEvent = Envelope & (
  | { type: "system"; subtype: "init"; cwd: string; model: string }
  | { type: "delta"; content: string }
  | { type: "message"; role: "assistant"; id: string; text: string }
  | { type: "reasoning"; text: string }
  | { type: "tool_call"; id: string; toolName: string; parameters: null | { [key: string]: Json }; server?: string }
  | { type: "tool_result"; id: string; toolName: string; isError: boolean; value: Json; durationMs: number; exitCode?: number }
  | { type: "completion"; finalText: string; numTurns: number; durationMs: number; toolCalls: number; success: true; error?: null }
);
export interface ClientOptions {
  /** Absolute path to a trusted Cortex executable; never a shell command. */
  executable: string;
  cwd: string;
  /** Trusted launcher arguments, for example for a controlled test adapter. */
  prefixArgs?: readonly string[];
  /** Omit to inherit the CLI's normal environment. Never logged by the client. */
  env?: Readonly<Record<string, string | undefined>>;
  maxPromptBytes?: number;
  maxLineBytes?: number;
  maxOutputBytes?: number;
  maxStderrBytes?: number;
  maxEvents?: number;
  killGraceMs?: number;
}
export interface TurnOptions {
  signal?: AbortSignal;
  timeoutMs?: number;
  /** Synchronous observer; events can contain user/code content. Not a log sink. */
  onEvent?: (event: CortexEvent) => void;
}
export interface TurnResult {
  readonly sessionId: string;
  readonly text: string;
  readonly numTurns: number;
  readonly durationMs: number;
  readonly toolCalls: number;
}
export interface Turn {
  readonly sessionId?: string;
  readonly result: Promise<TurnResult>;
  /** Waits for local child cleanup; does not confirm remote cancellation. */
  cancel(): Promise<void>;
}
export declare class CortexClient {
  constructor(options: ClientOptions);
  start(prompt: string, options?: TurnOptions): Turn;
  /** Fails if the CLI initializes a different session instead of resuming. */
  resume(sessionId: string, prompt: string, options?: TurnOptions): Turn;
}
