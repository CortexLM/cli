// Cortex protocol 1, dependency-free erasable TypeScript, Apache-2.0.
type Context = { session_id?: string; extra?: { call_id?: string } };
type Input = { tool: string; args: { name?: string } };
let calls: number = 0;

export default {
  protocol: 1,
  init: (_context: Context) => ({ data: null }),
  shutdown: (_context: Context) => ({ data: { calls } }),
  commands: {
    hello: (args: string[], context: Context) => ({
      data: { greeting: `Hello, ${args[0] ?? "world"}!`, calls: ++calls, session: context.session_id },
    }),
  },
  tools: {
    greet: (args: { name: string }, context: Context) => ({
      data: { greeting: `Hello, ${args.name}!`, calls: ++calls, call_id: context.extra?.call_id },
    }),
  },
  hooks: {
    session_start: (_input: unknown, _context: Context) => ({
      data: { decision: "continue" },
      notifications: [{ level: "info", message: "TypeScript plugin session started" }],
    }),
    tool_before: (input: Input) => {
      if (input.args?.name === "blocked") return { data: { decision: "deny", reason: "This name is blocked by the example plugin" } };
      return { data: { decision: "continue", input: { ...input, args: { ...input.args, name: input.args.name?.trim() } } } };
    },
    session_end: () => ({ data: { decision: "continue" } }),
  },
};
