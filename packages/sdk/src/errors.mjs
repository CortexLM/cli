const messages = Object.freeze({
  invalid_options: "Invalid Cortex client options",
  spawn_failed: "Could not start Cortex CLI",
  protocol_error: "Cortex CLI returned an invalid or incompatible event stream",
  output_limit: "Cortex CLI output exceeded the configured limit",
  execution_failed: "The coding service is temporarily unavailable",
  cancelled: "Cortex execution cancelled; remote cancellation is not confirmed",
  timeout: "Cortex execution timed out; remote cancellation is not confirmed",
  observer_failed: "The Cortex event observer failed",
});

export class CortexError extends Error {
  constructor(code, exitCode) {
    super(messages[code] ?? messages.protocol_error);
    this.name = "CortexError";
    this.code = code;
    if (exitCode !== undefined) this.exitCode = exitCode;
  }
}
