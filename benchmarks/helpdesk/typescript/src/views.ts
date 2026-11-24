// What each route answers: the record's public fields, never more. A member
// is shown without their email; a ticket names its customer by id only.
import type {
  customers,
  macros,
  memberships,
  replies,
  tags,
  tickets,
  ticketTags,
  users,
  workspaces,
} from "./db/schema.ts";

/** An instant as JSON carries it: UTC, `Z`, milliseconds only when there
 * are some (`2026-01-15T12:00:00Z`, `2026-01-15T12:00:00.250Z`). */
export function instant(value: Date): string {
  return value.toISOString().replace(".000Z", "Z");
}
const maybe = (value: Date | null) => (value ? instant(value) : null);

type Row<T extends { $inferSelect: unknown }> = T["$inferSelect"];

export const workspaceView = (row: Row<typeof workspaces>) => ({
  id: row.id,
  name: row.name,
  created_at: instant(row.created_at),
  updated_at: instant(row.updated_at),
});
export const meView = (row: Row<typeof users>) => ({
  id: row.id,
  email: row.email,
  status: row.status,
});
export const memberView = (row: Row<typeof memberships>) => ({
  id: row.id,
  user_id: row.user_id,
  role: row.role,
  display_name: row.display_name,
  created_at: instant(row.created_at),
  updated_at: instant(row.updated_at),
});
export const customerView = (row: Row<typeof customers>) => ({
  id: row.id,
  name: row.name,
  email: row.email,
  phone: row.phone,
  created_at: instant(row.created_at),
  updated_at: instant(row.updated_at),
});
export const ticketView = (row: Row<typeof tickets>) => ({
  id: row.id,
  customer_id: row.customer_id,
  assignee_id: row.assignee_id,
  subject: row.subject,
  status: row.status,
  priority: row.priority,
  respond_by: instant(row.respond_by),
  first_response_at: maybe(row.first_response_at),
  breached_at: maybe(row.breached_at),
  solved_at: maybe(row.solved_at),
  created_at: instant(row.created_at),
  updated_at: instant(row.updated_at),
});
export const messageView = (row: Row<typeof replies>) => ({
  id: row.id,
  ticket_id: row.ticket_id,
  author_id: row.author_id,
  body: row.body,
  created_at: instant(row.created_at),
  updated_at: instant(row.updated_at),
});
export const tagView = (row: Row<typeof tags>) => ({
  id: row.id,
  name: row.name,
  created_at: instant(row.created_at),
});
export const ticketTagView = (
  row: Row<typeof ticketTags>,
  tag: Row<typeof tags>,
) => ({
  id: row.id,
  created_at: instant(row.created_at),
  tag: tagView(tag),
});
export const macroView = (row: Row<typeof macros>) => ({
  id: row.id,
  title: row.title,
  body: row.body,
  created_at: instant(row.created_at),
  updated_at: instant(row.updated_at),
});
