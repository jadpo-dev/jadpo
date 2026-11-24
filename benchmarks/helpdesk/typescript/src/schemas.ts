// What each route reads and answers. Inputs are Zod schemas; a refused input
// is 422 `invalid_input` listing each problem as {field, rule}, never the
// value: `required`, `type`, `format`, `variant`, `min_length`,
// `max_length`, `min`, `max` and `unknown_field`.
import { z } from "zod";
import { PRIORITIES, ROLES, TICKET_STATUSES } from "./db/schema.ts";
import type { Problem } from "./errors.ts";
import { invalidInput } from "./errors.ts";

const rule = (name: string) => ({ params: { rule: name } });
const nfc = (value: unknown) =>
  typeof value === "string" ? value.normalize("NFC") : value;

/** Text composed to NFC, its length counted in Unicode characters. */
export const text = (min: number, max: number) =>
  z.preprocess(
    nfc,
    z.string().superRefine((value, context) => {
      const length = [...value].length;
      if (length < min)
        context.addIssue({ code: "custom", ...rule("min_length") });
      else if (length > max)
        context.addIssue({ code: "custom", ...rule("max_length") });
    }),
  );
export const Email = z.preprocess(
  nfc,
  z.string().refine((value) => {
    const at = value.indexOf("@");
    const domain = value.slice(at + 1);
    return (
      at > 0 &&
      !domain.includes("@") &&
      domain.includes(".") &&
      !domain.startsWith(".") &&
      !domain.endsWith(".") &&
      !/\s/u.test(value)
    );
  }, rule("format")),
);
/** Any UUID version, kept in lower case. */
export const Id = z
  .string()
  .regex(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i)
  .transform((value) => value.toLowerCase());

export const WorkspaceName = text(1, 80);
export const PersonLabel = text(1, 120);
export const PhoneText = text(5, 32);
export const TicketSubject = text(1, 200);
export const MessageBody = text(1, 5000);
export const TagName = text(1, 40);
export const MacroTitle = text(1, 80);
export const Role = z.enum(ROLES);
export const TicketStatus = z.enum(TICKET_STATUSES);
export const Priority = z.enum(PRIORITIES);

export const NewWorkspace = z.strictObject({
  name: WorkspaceName,
  display_name: PersonLabel,
});
export const WorkspaceRename = z.strictObject({ name: WorkspaceName });
export const NewMember = z.strictObject({
  user_id: Id,
  role: Role,
  display_name: PersonLabel,
});
// A patch leaves out what it doesn't change; `null` for a field that can't
// be empty also leaves it as it is.
export const MemberChange = z.strictObject({
  role: Role.nullish(),
  display_name: PersonLabel.nullish(),
});
export const CreateCustomer = z.strictObject({
  name: PersonLabel,
  email: Email,
  phone: PhoneText.nullish(),
});
export const PatchCustomer = z.strictObject({
  name: PersonLabel.nullish(),
  email: Email.nullish(),
  phone: PhoneText.nullish(),
});
export const CreateTicket = z.strictObject({
  customer_id: Id,
  subject: TicketSubject,
  priority: Priority.default("normal"),
});
export const PatchTicket = z.strictObject({
  subject: TicketSubject.nullish(),
  priority: Priority.nullish(),
});
// `assignee_id` must be sent: a member's id, or null to unassign.
export const Assignment = z.strictObject({ assignee_id: Id.nullable() });
export const WriteMessage = z.strictObject({ body: MessageBody });
export const NameTag = z.strictObject({ name: TagName });
export const AddTicketTag = z.strictObject({ tag_id: Id });
export const NewMacro = z.strictObject({
  title: MacroTitle,
  body: MessageBody,
});
export const PatchMacro = z.strictObject({
  title: MacroTitle.nullish(),
  body: MessageBody.nullish(),
});
// The query string of a ticket list: every value arrives as text.
const PageSize = z.string().transform((value, context) => {
  const size = /^[+-]?\d{1,18}$/.test(value) ? Number(value) : undefined;
  if (size === undefined) context.addIssue({ code: "custom", ...rule("type") });
  else if (size < 1) context.addIssue({ code: "custom", ...rule("min") });
  else if (size > 100) context.addIssue({ code: "custom", ...rule("max") });
  return size ?? z.NEVER;
});
export const ListTickets = z.strictObject({
  status: TicketStatus.optional(),
  assignee_id: Id.optional(),
  page_size: PageSize.optional(),
  after: z.string().optional(),
});
export type ListTickets = z.infer<typeof ListTickets>;

function valueAt(input: unknown, path: readonly PropertyKey[]): unknown {
  let current = input;
  for (const key of path) {
    if (current === null || typeof current !== "object") return undefined;
    current = (current as Record<PropertyKey, unknown>)[key];
  }
  return current;
}

/** The problems Zod found, as the client sees them: each field's in the
 * order the input declares them, then unknown fields by name. */
export function problemsOf(error: z.ZodError, input: unknown): Problem[] {
  const fields: Problem[] = [];
  const unknown: Problem[] = [];
  for (const issue of error.issues) {
    const at = issue.path.map(String);
    const field = at.join(".");
    switch (issue.code) {
      case "unrecognized_keys":
        for (const key of [...issue.keys].sort())
          unknown.push({
            field: [...at, key].join("."),
            rule: "unknown_field",
          });
        break;
      case "invalid_type": {
        const given = valueAt(input, issue.path);
        fields.push({
          field,
          rule: given === undefined || given === null ? "required" : "type",
        });
        break;
      }
      case "too_small":
        fields.push({
          field,
          rule: issue.origin === "number" ? "min" : "min_length",
        });
        break;
      case "too_big":
        fields.push({
          field,
          rule: issue.origin === "number" ? "max" : "max_length",
        });
        break;
      case "invalid_value": {
        const given = valueAt(input, issue.path);
        fields.push({
          field,
          rule:
            given === undefined || given === null
              ? "required"
              : typeof given === "string"
                ? "variant"
                : "type",
        });
        break;
      }
      case "custom": {
        const named = (issue.params as { rule?: unknown } | undefined)?.rule;
        fields.push({
          field,
          rule: typeof named === "string" ? named : "format",
        });
        break;
      }
      default:
        fields.push({ field, rule: "format" });
    }
  }
  return [...fields, ...unknown];
}

/** `value` read by `schema`, or 422 `invalid_input` with its problems. */
export function decode<T extends z.ZodType>(
  schema: T,
  value: unknown,
): z.output<T> {
  const result = schema.safeParse(value);
  if (!result.success) throw invalidInput(problemsOf(result.error, value));
  return result.data;
}
