export const failures = {
  idempotency_key_reused: [422, "Idempotency key reused."],
  conflict: [409, "Conflict."],
  authentication_required: [401, "Authentication required."],
  sign_in_refused: [403, "Sign-in refused."],
  invalid_credentials: [401, "Invalid credentials."],
  ambiguous_credentials: [401, "Ambiguous credentials."],
  csrf_rejected: [403, "Request verification failed."],
  user_disabled: [403, "User disabled."],
  service_disabled: [403, "Service disabled."],
  not_permitted: [403, "Not permitted."],
  request_too_large: [413, "Request too large."],
  invalid_request: [400, "Invalid request."],
  invalid_input: [422, "Invalid input."],
  due_date_in_past: [422, "The due date must be in the future."],
  todo_not_found: [404, "Todo not found."],
  user_not_found: [404, "User not found."],
  route_not_found: [404, "Route not found."],
  method_not_allowed: [405, "Method not allowed."],
  rate_limited: [429, "Too many requests."],
  unavailable: [503, "Unavailable."],
  authentication_misconfigured: [500, "Authentication misconfigured."],
  authentication_unavailable: [503, "Authentication unavailable."],
  deadline_exceeded: [504, "Request deadline exceeded."],
  internal_fault: [500, "Internal fault."],
  relationship_conflict: [409, "Relationship conflict."],
} as const;
export const newRequestId = () =>
  "req_" + crypto.randomUUID().replaceAll("-", "").slice(0, 16);
export type FailureCode = keyof typeof failures;
export class AppError extends Error {
  readonly code: FailureCode;
  constructor(code: FailureCode) {
    super(failures[code][1]);
    this.code = code;
  }
}
export function publicFailure(error: unknown, requestId: string) {
  const code = error instanceof AppError ? error.code : "internal_fault";
  return {
    status: failures[code][0],
    body: {
      error: { code, message: failures[code][1], request_id: requestId },
    },
  };
}
// Closed log events accept correlation and safe classifications, never exception text,
// arbitrary objects, request input, database details, provider bodies or configuration.
export type LogEvent = Readonly<{
  event: "request_failed" | "job_failed" | "configuration_refused";
  request_id: string;
  code: string;
}>;
export type Logger = (event: LogEvent) => void;
export function safeLogRecord(event: LogEvent) {
  return {
    event: ["request_failed", "job_failed", "configuration_refused"].includes(
      event.event,
    )
      ? event.event
      : "request_failed",
    request_id: /^(?:[a-f0-9-]{36}|req_[a-f0-9]{16})$/.test(event.request_id)
      ? event.request_id
      : "redacted",
    code:
      Object.hasOwn(failures, event.code) || event.code === "provider_refused"
        ? event.code
        : "internal_fault",
  };
}
export const structuredLog: Logger = (event) => {
  process.stderr.write(JSON.stringify(safeLogRecord(event)) + "\n");
};
