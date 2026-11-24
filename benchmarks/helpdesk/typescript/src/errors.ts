// Every failure a client can see: its status and fixed English message. The
// envelope is {"error":{"code","message","request_id","details"?}} with the
// same id in the x-request-id header.
export const failures = {
  // Request handling.
  invalid_request: [400, "Request could not be read."],
  invalid_input: [422, "Request input is invalid."],
  route_not_found: [404, "No route matches this request."],
  method_not_allowed: [405, "This path doesn't answer this method."],
  request_too_large: [413, "The request body is over 1 MB."],
  unavailable: [503, "A dependency is temporarily unavailable."],
  internal_fault: [500, "The server failed to answer the request."],
  // Sign-in.
  authentication_required: [401, "Sign in first."],
  invalid_credentials: [401, "Invalid credentials."],
  ambiguous_credentials: [401, "Ambiguous credentials."],
  csrf_rejected: [403, "Request verification failed."],
  user_disabled: [403, "User disabled."],
  authentication_unavailable: [503, "Authentication unavailable."],
  // Policy: a record the caller may see but not change, or a create.
  not_permitted: [403, "Not permitted."],
  // Records that don't exist, or that the caller may not see.
  workspace_not_found: [404, "Not found."],
  user_not_found: [404, "Not found."],
  membership_not_found: [404, "Not found."],
  customer_not_found: [404, "Not found."],
  ticket_not_found: [404, "Not found."],
  reply_not_found: [404, "Not found."],
  note_not_found: [404, "Not found."],
  tag_not_found: [404, "Not found."],
  ticket_tag_not_found: [404, "Not found."],
  macro_not_found: [404, "Not found."],
  // The desk's own rules.
  already_member: [409, "That person is already a member of this workspace."],
  already_tagged: [409, "The ticket already has that tag."],
  customer_email_taken: [
    409,
    "This workspace already has a customer with that email.",
  ],
  tag_name_taken: [409, "This workspace already has a tag with that name."],
  assignee_cannot_work: [
    422,
    "Tickets can only be assigned to owners, admins and agents.",
  ],
  last_owner: [422, "A workspace must keep at least one owner."],
} as const;
export type FailureCode = keyof typeof failures;

/** One broken input rule: the field's path and the rule, never the value. */
export type Problem = { field: string; rule: string };

export class AppError extends Error {
  readonly code: FailureCode;
  readonly problems: readonly Problem[] | undefined;
  constructor(code: FailureCode, problems?: readonly Problem[]) {
    super(failures[code][1]);
    this.code = code;
    this.problems = problems;
  }
}

export const invalidInput = (problems: readonly Problem[]) =>
  new AppError("invalid_input", problems);

export const newRequestId = () =>
  "req_" + crypto.randomUUID().replaceAll("-", "").slice(0, 16);

export function publicFailure(error: unknown, requestId: string) {
  const code: FailureCode =
    error instanceof AppError ? error.code : "internal_fault";
  const problems = error instanceof AppError ? error.problems : undefined;
  return {
    status: failures[code][0],
    body: {
      error: {
        code,
        message: failures[code][1],
        request_id: requestId,
        ...(problems ? { details: { problems } } : {}),
      },
    },
  };
}

// Log lines carry a request id and a failure code, never input, records,
// credentials or exception text.
export type LogEvent = Readonly<{
  event: "request_failed" | "job_failed" | "configuration_refused";
  request_id: string;
  code: string;
}>;
export type Logger = (event: LogEvent) => void;
export const structuredLog: Logger = (event) => {
  process.stderr.write(JSON.stringify(event) + "\n");
};
