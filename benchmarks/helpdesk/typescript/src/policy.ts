// Who may do what, per record kind, by the caller's role in the record's
// workspace. A caller with no membership there has no role, so nothing of
// one workspace reaches a member of another.
//
// How a refusal looks:
// - a record the caller may not read, read or changed by id: its own
//   `<kind>_not_found`, as if it didn't exist;
// - a list: only what the caller may read, so another workspace's is empty;
// - a record the caller may read but not change, or a create: 403
//   `not_permitted`.
import { and, eq, exists, inArray, sql } from "drizzle-orm";
import type { SQL } from "drizzle-orm";
import type { AnyPgColumn } from "drizzle-orm/pg-core";
import { alias } from "drizzle-orm/pg-core";
import { memberships } from "./db/schema.ts";
import type { Role } from "./db/schema.ts";
import type { Tx } from "./db/store.ts";
import { AppError } from "./errors.ts";

const ALL: readonly Role[] = ["owner", "admin", "agent", "viewer"];
const STAFF: readonly Role[] = ["owner", "admin", "agent"];
const MANAGERS: readonly Role[] = ["owner", "admin"];
const OWNERS: readonly Role[] = ["owner"];

type Grants = Readonly<{
  create?: readonly Role[];
  read: readonly Role[];
  update?: readonly Role[];
  delete?: readonly Role[];
  /** Whether a record's author may delete it whatever their role. */
  authorDeletes?: boolean;
}>;

export const grants = {
  workspace: { read: ALL, update: OWNERS },
  membership: { create: OWNERS, read: ALL, update: OWNERS, delete: OWNERS },
  customer: { create: STAFF, read: STAFF, update: STAFF, delete: MANAGERS },
  ticket: { create: STAFF, read: ALL, update: STAFF, delete: MANAGERS },
  reply: {
    create: STAFF,
    read: ALL,
    update: STAFF,
    delete: MANAGERS,
    authorDeletes: true,
  },
  note: {
    create: STAFF,
    read: STAFF,
    update: STAFF,
    delete: MANAGERS,
    authorDeletes: true,
  },
  tag: { create: MANAGERS, read: ALL, update: MANAGERS, delete: MANAGERS },
  ticketTag: { create: STAFF, read: ALL, delete: STAFF },
  macro: { create: MANAGERS, read: STAFF, update: MANAGERS, delete: MANAGERS },
} as const satisfies Record<string, Grants>;
export type Kind = keyof typeof grants;
export type Operation = "create" | "read" | "update" | "delete";

function rolesFor(kind: Kind, operation: Operation): readonly Role[] {
  const entry: Grants = grants[kind];
  return entry[operation] ?? [];
}

const caller = alias(memberships, "caller_membership");
/** SQL: the caller holds one of the roles `operation` needs in the
 * workspace `workspace` names. For list and by-id filters. */
export function granted(
  tx: Tx,
  kind: Kind,
  operation: Operation,
  user: string,
  workspace: AnyPgColumn | SQL,
): SQL {
  const roles = rolesFor(kind, operation);
  if (!roles.length) return sql`false`;
  return exists(
    tx
      .select({ one: sql`1` })
      .from(caller)
      .where(
        and(
          eq(caller.workspace_id, workspace),
          eq(caller.user_id, user),
          inArray(caller.role, [...roles]),
        ),
      ),
  );
}

/** The caller's role in a workspace, if they belong to it. */
export async function roleIn(
  tx: Tx,
  user: string,
  workspace: string,
): Promise<Role | undefined> {
  const rows = await tx
    .select({ role: memberships.role })
    .from(memberships)
    .where(
      and(
        eq(memberships.workspace_id, workspace),
        eq(memberships.user_id, user),
      ),
    )
    .limit(1);
  return rows[0]?.role;
}

/** 403 `not_permitted` unless the caller may do `operation` to a record of
 * `kind` in `workspace` (or wrote it, where its author may delete it). */
export async function permit(
  tx: Tx,
  kind: Kind,
  operation: Operation,
  user: string,
  workspace: string,
  author?: string | null,
): Promise<void> {
  const role = await roleIn(tx, user, workspace);
  const entry: Grants = grants[kind];
  const byAuthor =
    operation === "delete" &&
    entry.authorDeletes === true &&
    role !== undefined &&
    author === user;
  if (byAuthor) return;
  if (!role || !rolesFor(kind, operation).includes(role))
    throw new AppError("not_permitted");
}

/** A reference from a record of `workspace` to another record must stay in
 * that workspace: 403 `not_permitted` when it points into another one. */
export function sameWorkspace(workspace: string, referenced: string): void {
  if (workspace !== referenced) throw new AppError("not_permitted");
}
