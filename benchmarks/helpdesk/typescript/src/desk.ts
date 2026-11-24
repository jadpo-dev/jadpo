// The desk's actions. Each runs in one serializable transaction, as the
// signed-in caller, with the policy of src/policy.ts. Times come from the
// injected clock, so every timestamp a request writes is the same instant.
import {
  and,
  asc,
  desc,
  eq,
  inArray,
  isNull,
  lt,
  lte,
  or,
  sql,
} from "drizzle-orm";
import { v7 } from "uuid";
import type { Principal } from "./auth.ts";
import type { Config } from "./config.ts";
import {
  customers,
  macros,
  memberships,
  notes,
  replies,
  sessions,
  tags,
  tickets,
  ticketTags,
  users,
  workspaces,
} from "./db/schema.ts";
import type { Priority, Role } from "./db/schema.ts";
import { brokenConstraint } from "./db/store.ts";
import type { Store, Tx } from "./db/store.ts";
import { AppError, invalidInput } from "./errors.ts";
import type { FailureCode } from "./errors.ts";
import { granted, permit, sameWorkspace } from "./policy.ts";
import type { Kind } from "./policy.ts";
import type { ListTickets } from "./schemas.ts";
import {
  customerView,
  macroView,
  memberView,
  meView,
  messageView,
  tagView,
  ticketTagView,
  ticketView,
  workspaceView,
} from "./views.ts";

/** Replies and notes share one shape. */
type Messages = typeof replies;

/** A patch: a field left out, or `null` where the field can't be empty,
 * leaves it as it is. */
type Patch<T> = { [K in keyof T]?: T[K] | null | undefined };

function first<T>(rows: readonly T[], code: FailureCode): T {
  const row = rows[0];
  if (!row) throw new AppError(code);
  return row;
}

/** Runs `write`, turning a broken unique constraint into its failure. */
async function unique<T>(
  write: Promise<T>,
  conflicts: Record<string, FailureCode>,
): Promise<T> {
  try {
    return await write;
  } catch (error) {
    const constraint = brokenConstraint(error, "23505");
    const code = constraint ? conflicts[constraint] : undefined;
    if (code) throw new AppError(code);
    throw error;
  }
}

/** A patch that sets nothing writes nothing: `updated_at` stays. */
const unchanged = (values: object) => Object.keys(values).length === 0;

/** The fields of `patch` that change something. */
function changes<T extends object>(patch: Patch<T>, nullable: string[] = []) {
  return Object.fromEntries(
    Object.entries(patch).filter(
      ([key, value]) =>
        value !== undefined && (value !== null || nullable.includes(key)),
    ),
  ) as Partial<T>;
}

// A page cursor: the last ticket's creation time and id, opaque to clients.
function cursorOf(row: { created_at: Date; id: string }): string {
  return Buffer.from(
    JSON.stringify([row.created_at.getTime(), row.id]),
  ).toString("base64url");
}
function readCursor(text: string): { at: Date; id: string } {
  try {
    const value: unknown = JSON.parse(
      Buffer.from(text, "base64url").toString("utf8"),
    );
    if (
      Array.isArray(value) &&
      value.length === 2 &&
      Number.isSafeInteger(value[0]) &&
      typeof value[1] === "string" &&
      /^[0-9a-f-]{36}$/.test(value[1])
    )
      return { at: new Date(value[0] as number), id: value[1] };
  } catch {
    /* Refused below. */
  }
  throw invalidInput([{ field: "after", rule: "format" }]);
}

export class Desk {
  readonly store: Store;
  readonly clock: () => number;
  readonly config: Config;
  constructor(store: Store, clock: () => number, config: Config) {
    this.store = store;
    this.clock = clock;
    this.config = config;
  }

  private now(): Date {
    return new Date(this.clock());
  }
  private id(): string {
    return v7({ msecs: this.clock() });
  }
  private run<T>(body: (tx: Tx) => Promise<T>): Promise<T> {
    return this.store.transaction(body);
  }

  // Records by id, as the caller may see them -----------------------------

  private async workspaceRow(
    tx: Tx,
    user: Principal,
    id: string,
    operation: "read" | "update" = "read",
  ) {
    const rows = await tx
      .select()
      .from(workspaces)
      .where(
        and(
          eq(workspaces.id, id),
          granted(tx, "workspace", operation, user.id, workspaces.id),
        ),
      );
    return first(rows, "workspace_not_found");
  }
  private async membershipRow(tx: Tx, user: Principal, id: string) {
    const rows = await tx
      .select()
      .from(memberships)
      .where(
        and(
          eq(memberships.id, id),
          granted(tx, "membership", "read", user.id, memberships.workspace_id),
        ),
      );
    return first(rows, "membership_not_found");
  }
  private async customerRow(tx: Tx, user: Principal, id: string) {
    const rows = await tx
      .select()
      .from(customers)
      .where(
        and(
          eq(customers.id, id),
          granted(tx, "customer", "read", user.id, customers.workspace_id),
        ),
      );
    return first(rows, "customer_not_found");
  }
  /** A ticket in the bin is gone, to everyone. */
  private async ticketRow(tx: Tx, user: Principal, id: string) {
    const rows = await tx
      .select()
      .from(tickets)
      .where(
        and(
          eq(tickets.id, id),
          isNull(tickets.deleted_at),
          granted(tx, "ticket", "read", user.id, tickets.workspace_id),
        ),
      );
    return first(rows, "ticket_not_found");
  }
  private async tagRow(
    tx: Tx,
    user: Principal,
    id: string,
    operation: "read" | "update" = "read",
  ) {
    const rows = await tx
      .select()
      .from(tags)
      .where(
        and(
          eq(tags.id, id),
          granted(tx, "tag", operation, user.id, tags.workspace_id),
        ),
      );
    return first(rows, "tag_not_found");
  }
  private async macroRow(tx: Tx, user: Principal, id: string) {
    const rows = await tx
      .select()
      .from(macros)
      .where(
        and(
          eq(macros.id, id),
          granted(tx, "macro", "read", user.id, macros.workspace_id),
        ),
      );
    return first(rows, "macro_not_found");
  }
  private async messageRow(
    tx: Tx,
    table: Messages,
    user: Principal,
    id: string,
    operation: "read" | "update",
  ) {
    const kind = table === replies ? "reply" : "note";
    const rows = await tx
      .select()
      .from(table)
      .where(
        and(
          eq(table.id, id),
          granted(tx, kind, operation, user.id, table.workspace_id),
        ),
      );
    return first(rows, kind === "reply" ? "reply_not_found" : "note_not_found");
  }

  // A create first needs the caller's create grant in the new record's
  // workspace (none in a workspace that doesn't exist): else 403. Then each
  // record it refers to must exist (else that record's not-found failure)
  // and be in the same workspace (else 403).
  private async creating(
    tx: Tx,
    kind: Kind,
    user: Principal,
    workspace: string,
  ): Promise<void> {
    await permit(tx, kind, "create", user.id, workspace);
  }

  // Workspaces -----------------------------------------------------------

  async foundWorkspace(
    user: Principal,
    input: { name: string; display_name: string },
  ) {
    return await this.run(async (tx) => {
      const now = this.now();
      const [workspace] = await tx
        .insert(workspaces)
        .values({
          id: this.id(),
          name: input.name,
          created_at: now,
          updated_at: now,
        })
        .returning();
      if (!workspace) throw new AppError("internal_fault");
      // The founder is its first owner: no one else could grant it.
      await unique(
        tx.insert(memberships).values({
          id: this.id(),
          workspace_id: workspace.id,
          user_id: user.id,
          role: "owner",
          display_name: input.display_name,
          created_at: now,
          updated_at: now,
        }),
        { membership_one_per_person_key: "already_member" },
      );
      return workspaceView(workspace);
    });
  }

  async myWorkspaces(user: Principal) {
    return await this.run(async (tx) => {
      const rows = await tx
        .select()
        .from(workspaces)
        .where(granted(tx, "workspace", "read", user.id, workspaces.id))
        .orderBy(asc(workspaces.created_at), asc(workspaces.id))
        .limit(100);
      return rows.map(workspaceView);
    });
  }

  async workspace(user: Principal, id: string) {
    return await this.run(async (tx) =>
      workspaceView(await this.workspaceRow(tx, user, id)),
    );
  }

  /** Only owners may; anyone else is told there is no such workspace. */
  async renameWorkspace(user: Principal, id: string, input: { name: string }) {
    return await this.run(async (tx) => {
      await this.workspaceRow(tx, user, id, "update");
      const [changed] = await tx
        .update(workspaces)
        .set({ name: input.name, updated_at: this.now() })
        .where(eq(workspaces.id, id))
        .returning();
      return workspaceView(
        first(changed ? [changed] : [], "workspace_not_found"),
      );
    });
  }

  async me(user: Principal) {
    return await this.run(async (tx) => {
      const rows = await tx.select().from(users).where(eq(users.id, user.id));
      return meView(first(rows, "user_not_found"));
    });
  }

  // Members --------------------------------------------------------------

  async members(user: Principal, workspace: string) {
    return await this.run(async (tx) => {
      const rows = await tx
        .select()
        .from(memberships)
        .where(
          and(
            eq(memberships.workspace_id, workspace),
            granted(
              tx,
              "membership",
              "read",
              user.id,
              memberships.workspace_id,
            ),
          ),
        )
        .orderBy(asc(memberships.created_at), asc(memberships.id))
        .limit(200);
      return rows.map(memberView);
    });
  }

  async addMember(
    user: Principal,
    workspace: string,
    input: { user_id: string; role: Role; display_name: string },
  ) {
    return await this.run(async (tx) => {
      await this.creating(tx, "membership", user, workspace);
      // People belong to no workspace: the new member need only exist.
      const person = await tx
        .select({ id: users.id })
        .from(users)
        .where(eq(users.id, input.user_id));
      first(person, "user_not_found");
      const now = this.now();
      const [member] = await unique(
        tx
          .insert(memberships)
          .values({
            id: this.id(),
            workspace_id: workspace,
            user_id: input.user_id,
            role: input.role,
            display_name: input.display_name,
            created_at: now,
            updated_at: now,
          })
          .returning(),
        { membership_one_per_person_key: "already_member" },
      );
      return memberView(first(member ? [member] : [], "internal_fault"));
    });
  }

  /** Fewer than two owners means this one is the last. */
  private async lastOwner(tx: Tx, workspace: string): Promise<boolean> {
    const owners = await tx
      .select({ id: memberships.id })
      .from(memberships)
      .where(
        and(
          eq(memberships.workspace_id, workspace),
          eq(memberships.role, "owner"),
        ),
      )
      .orderBy(asc(memberships.id))
      .limit(2);
    return owners.length < 2;
  }

  async changeMember(
    user: Principal,
    id: string,
    input: Patch<{ role: Role; display_name: string }>,
  ) {
    return await this.run(async (tx) => {
      const member = await this.membershipRow(tx, user, id);
      if (
        input.role &&
        member.role === "owner" &&
        input.role !== "owner" &&
        (await this.lastOwner(tx, member.workspace_id))
      )
        throw new AppError("last_owner");
      await permit(tx, "membership", "update", user.id, member.workspace_id);
      const values = changes(input);
      if (unchanged(values)) return memberView(member);
      const [changed] = await tx
        .update(memberships)
        .set({ ...values, updated_at: this.now() })
        .where(eq(memberships.id, id))
        .returning();
      return memberView(
        first(changed ? [changed] : [], "membership_not_found"),
      );
    });
  }

  /** Their tickets become unassigned (the foreign key clears them). */
  async removeMember(user: Principal, id: string) {
    await this.run(async (tx) => {
      const member = await this.membershipRow(tx, user, id);
      if (
        member.role === "owner" &&
        (await this.lastOwner(tx, member.workspace_id))
      )
        throw new AppError("last_owner");
      await permit(tx, "membership", "delete", user.id, member.workspace_id);
      await tx.delete(memberships).where(eq(memberships.id, id));
    });
  }

  // Customers ------------------------------------------------------------

  async customers(user: Principal, workspace: string) {
    return await this.run(async (tx) => {
      const rows = await tx
        .select()
        .from(customers)
        .where(
          and(
            eq(customers.workspace_id, workspace),
            granted(tx, "customer", "read", user.id, customers.workspace_id),
          ),
        )
        .orderBy(asc(customers.created_at), asc(customers.id))
        .limit(200);
      return rows.map(customerView);
    });
  }

  async addCustomer(
    user: Principal,
    workspace: string,
    input: { name: string; email: string; phone?: string | null | undefined },
  ) {
    return await this.run(async (tx) => {
      await this.creating(tx, "customer", user, workspace);
      const now = this.now();
      const [customer] = await unique(
        tx
          .insert(customers)
          .values({
            id: this.id(),
            workspace_id: workspace,
            name: input.name,
            email: input.email,
            phone: input.phone ?? null,
            created_at: now,
            updated_at: now,
          })
          .returning(),
        { customer_email_key: "customer_email_taken" },
      );
      return customerView(first(customer ? [customer] : [], "internal_fault"));
    });
  }

  async customer(user: Principal, id: string) {
    return await this.run(async (tx) =>
      customerView(await this.customerRow(tx, user, id)),
    );
  }

  async editCustomer(
    user: Principal,
    id: string,
    input: Patch<{ name: string; email: string; phone: string | null }>,
  ) {
    return await this.run(async (tx) => {
      const customer = await this.customerRow(tx, user, id);
      await permit(tx, "customer", "update", user.id, customer.workspace_id);
      const values = changes(input, ["phone"]);
      if (unchanged(values)) return customerView(customer);
      const [changed] = await unique(
        tx
          .update(customers)
          .set({ ...values, updated_at: this.now() })
          .where(eq(customers.id, id))
          .returning(),
        { customer_email_key: "customer_email_taken" },
      );
      return customerView(
        first(changed ? [changed] : [], "customer_not_found"),
      );
    });
  }

  /** Their tickets stay, without them (the foreign key clears them). */
  async deleteCustomer(user: Principal, id: string) {
    await this.run(async (tx) => {
      const customer = await this.customerRow(tx, user, id);
      await permit(tx, "customer", "delete", user.id, customer.workspace_id);
      await tx.delete(customers).where(eq(customers.id, id));
    });
  }

  /** A customer's tickets, newest first, as the caller may read tickets. */
  async customerTickets(user: Principal, customer: string) {
    return await this.run(async (tx) => {
      const rows = await tx
        .select()
        .from(tickets)
        .where(
          and(
            eq(tickets.customer_id, customer),
            isNull(tickets.deleted_at),
            granted(tx, "ticket", "read", user.id, tickets.workspace_id),
          ),
        )
        .orderBy(desc(tickets.created_at), desc(tickets.id))
        .limit(100);
      return rows.map(ticketView);
    });
  }

  // Tickets --------------------------------------------------------------

  async ticketsPage(user: Principal, workspace: string, input: ListTickets) {
    const size = input.page_size ?? 25;
    const after =
      input.after === undefined ? undefined : readCursor(input.after);
    return await this.run(async (tx) => {
      const rows = await tx
        .select()
        .from(tickets)
        .where(
          and(
            eq(tickets.workspace_id, workspace),
            isNull(tickets.deleted_at),
            granted(tx, "ticket", "read", user.id, tickets.workspace_id),
            input.status ? eq(tickets.status, input.status) : undefined,
            input.assignee_id
              ? eq(tickets.assignee_id, input.assignee_id)
              : undefined,
            after
              ? or(
                  lt(tickets.created_at, after.at),
                  and(
                    eq(tickets.created_at, after.at),
                    lt(tickets.id, after.id),
                  ),
                )
              : undefined,
          ),
        )
        .orderBy(desc(tickets.created_at), desc(tickets.id))
        .limit(size + 1);
      const page = rows.slice(0, size);
      const last = page.at(-1);
      return {
        items: page.map(ticketView),
        next: rows.length > size && last ? cursorOf(last) : null,
      };
    });
  }

  async openTicket(
    user: Principal,
    workspace: string,
    input: { customer_id: string; subject: string; priority: Priority },
  ) {
    return await this.run(async (tx) => {
      await this.creating(tx, "ticket", user, workspace);
      const found = await tx
        .select({ workspace: customers.workspace_id })
        .from(customers)
        .where(eq(customers.id, input.customer_id));
      const customer = first(found, "customer_not_found");
      sameWorkspace(workspace, customer.workspace);
      const now = this.now();
      const [ticket] = await tx
        .insert(tickets)
        .values({
          id: this.id(),
          workspace_id: workspace,
          customer_id: input.customer_id,
          assignee_id: null,
          subject: input.subject,
          status: "open",
          priority: input.priority,
          respond_by: new Date(
            now.getTime() + this.config.firstResponseTargetMs,
          ),
          first_response_at: null,
          breached_at: null,
          solved_at: null,
          deleted_at: null,
          created_at: now,
          updated_at: now,
        })
        .returning();
      return ticketView(first(ticket ? [ticket] : [], "internal_fault"));
    });
  }

  async ticket(user: Principal, id: string) {
    return await this.run(async (tx) =>
      ticketView(await this.ticketRow(tx, user, id)),
    );
  }

  private async changeTicket(
    tx: Tx,
    user: Principal,
    ticket: typeof tickets.$inferSelect,
    values: Partial<typeof tickets.$inferInsert>,
  ) {
    await permit(tx, "ticket", "update", user.id, ticket.workspace_id);
    if (unchanged(values)) return ticket;
    const [changed] = await tx
      .update(tickets)
      .set({ ...values, updated_at: this.now() })
      .where(eq(tickets.id, ticket.id))
      .returning();
    return first(changed ? [changed] : [], "ticket_not_found");
  }

  async editTicket(
    user: Principal,
    id: string,
    input: Patch<{ subject: string; priority: Priority }>,
  ) {
    return await this.run(async (tx) => {
      const ticket = await this.ticketRow(tx, user, id);
      return ticketView(
        await this.changeTicket(tx, user, ticket, changes(input)),
      );
    });
  }

  /** Moves a ticket to the bin; it is purged after the retention period. */
  async deleteTicket(user: Principal, id: string) {
    await this.run(async (tx) => {
      const ticket = await this.ticketRow(tx, user, id);
      await permit(tx, "ticket", "delete", user.id, ticket.workspace_id);
      const now = this.now();
      await tx
        .update(tickets)
        .set({ deleted_at: now, updated_at: now })
        .where(and(eq(tickets.id, id), isNull(tickets.deleted_at)));
    });
  }

  /** Viewers can't be given tickets; nobody (null) unassigns it. */
  async assignTicket(
    user: Principal,
    id: string,
    input: { assignee_id: string | null },
  ) {
    return await this.run(async (tx) => {
      const ticket = await this.ticketRow(tx, user, id);
      if (input.assignee_id !== null) {
        const assignee = await this.membershipRow(tx, user, input.assignee_id);
        if (assignee.role === "viewer")
          throw new AppError("assignee_cannot_work");
        await permit(tx, "ticket", "update", user.id, ticket.workspace_id);
        sameWorkspace(ticket.workspace_id, assignee.workspace_id);
      }
      return ticketView(
        await this.changeTicket(tx, user, ticket, {
          assignee_id: input.assignee_id,
        }),
      );
    });
  }

  /** Solving a solved ticket changes nothing. */
  async solveTicket(user: Principal, id: string) {
    return await this.run(async (tx) => {
      const ticket = await this.ticketRow(tx, user, id);
      if (ticket.status === "solved") return ticketView(ticket);
      return ticketView(
        await this.changeTicket(tx, user, ticket, {
          status: "solved",
          solved_at: this.now(),
        }),
      );
    });
  }

  async reopenTicket(user: Principal, id: string) {
    return await this.run(async (tx) => {
      const ticket = await this.ticketRow(tx, user, id);
      return ticketView(
        await this.changeTicket(tx, user, ticket, {
          status: "open",
          solved_at: null,
        }),
      );
    });
  }

  /** The ticket's customer, if the caller may see customers. */
  async ticketCustomer(user: Principal, id: string) {
    return await this.run(async (tx) => {
      const ticket = await this.ticketRow(tx, user, id);
      if (!ticket.customer_id) throw new AppError("customer_not_found");
      return customerView(await this.customerRow(tx, user, ticket.customer_id));
    });
  }

  // Replies and notes ----------------------------------------------------

  private async messages(table: Messages, user: Principal, ticketId: string) {
    const kind = table === replies ? "reply" : "note";
    return await this.run(async (tx) => {
      const ticket = await this.ticketRow(tx, user, ticketId);
      const rows = await tx
        .select()
        .from(table)
        .where(
          and(
            eq(table.ticket_id, ticket.id),
            granted(tx, kind, "read", user.id, table.workspace_id),
          ),
        )
        .orderBy(asc(table.created_at), asc(table.id))
        .limit(500);
      return rows.map(messageView);
    });
  }

  private async write(
    tx: Tx,
    table: Messages,
    user: Principal,
    ticket: { id: string; workspace_id: string },
    body: string,
  ) {
    await permit(
      tx,
      table === replies ? "reply" : "note",
      "create",
      user.id,
      ticket.workspace_id,
    );
    const now = this.now();
    const [written] = await tx
      .insert(table)
      .values({
        id: this.id(),
        workspace_id: ticket.workspace_id,
        ticket_id: ticket.id,
        author_id: user.id,
        body,
        created_at: now,
        updated_at: now,
      })
      .returning();
    return messageView(first(written ? [written] : [], "internal_fault"));
  }

  async ticketReplies(user: Principal, ticket: string) {
    return await this.messages(replies, user, ticket);
  }

  /** The first answer to an open ticket stops its first-response clock,
   * and the ticket then waits on the customer. */
  async addReply(user: Principal, ticketId: string, input: { body: string }) {
    return await this.run(async (tx) => {
      const ticket = await this.ticketRow(tx, user, ticketId);
      const reply = await this.write(tx, replies, user, ticket, input.body);
      if (ticket.status === "open")
        await this.changeTicket(tx, user, ticket, {
          status: "pending",
          first_response_at: ticket.first_response_at ?? this.now(),
        });
      return reply;
    });
  }

  private async editMessage(
    table: Messages,
    user: Principal,
    id: string,
    body: string,
  ) {
    return await this.run(async (tx) => {
      // Changed by id: a message the caller may not change isn't there.
      await this.messageRow(tx, table, user, id, "update");
      const [changed] = await tx
        .update(table)
        .set({ body, updated_at: this.now() })
        .where(eq(table.id, id))
        .returning();
      return messageView(
        first(
          changed ? [changed] : [],
          table === replies ? "reply_not_found" : "note_not_found",
        ),
      );
    });
  }

  /** Its author, owners and admins may delete a message. */
  private async deleteMessage(table: Messages, user: Principal, id: string) {
    await this.run(async (tx) => {
      const message = await this.messageRow(tx, table, user, id, "read");
      await permit(
        tx,
        table === replies ? "reply" : "note",
        "delete",
        user.id,
        message.workspace_id,
        message.author_id,
      );
      await tx.delete(table).where(eq(table.id, id));
    });
  }

  async editReply(user: Principal, id: string, input: { body: string }) {
    return await this.editMessage(replies, user, id, input.body);
  }
  async deleteReply(user: Principal, id: string) {
    await this.deleteMessage(replies, user, id);
  }

  async ticketNotes(user: Principal, ticket: string) {
    return await this.messages(notes, user, ticket);
  }
  async addNote(user: Principal, ticketId: string, input: { body: string }) {
    return await this.run(async (tx) => {
      const ticket = await this.ticketRow(tx, user, ticketId);
      return await this.write(tx, notes, user, ticket, input.body);
    });
  }
  async editNote(user: Principal, id: string, input: { body: string }) {
    return await this.editMessage(notes, user, id, input.body);
  }
  async deleteNote(user: Principal, id: string) {
    await this.deleteMessage(notes, user, id);
  }

  // Tags -----------------------------------------------------------------

  async tags(user: Principal, workspace: string) {
    return await this.run(async (tx) => {
      const rows = await tx
        .select()
        .from(tags)
        .where(
          and(
            eq(tags.workspace_id, workspace),
            granted(tx, "tag", "read", user.id, tags.workspace_id),
          ),
        )
        .orderBy(sql`${tags.name} COLLATE "C"`, asc(tags.id))
        .limit(200);
      return rows.map(tagView);
    });
  }

  async createTag(user: Principal, workspace: string, input: { name: string }) {
    return await this.run(async (tx) => {
      await this.creating(tx, "tag", user, workspace);
      const [tag] = await unique(
        tx
          .insert(tags)
          .values({
            id: this.id(),
            workspace_id: workspace,
            name: input.name,
            created_at: this.now(),
          })
          .returning(),
        { tag_name_key: "tag_name_taken" },
      );
      return tagView(first(tag ? [tag] : [], "internal_fault"));
    });
  }

  /** Renamed by id: a tag the caller may not change isn't there. */
  async renameTag(user: Principal, id: string, input: { name: string }) {
    return await this.run(async (tx) => {
      await this.tagRow(tx, user, id, "update");
      const [tag] = await unique(
        tx
          .update(tags)
          .set({ name: input.name })
          .where(eq(tags.id, id))
          .returning(),
        { tag_name_key: "tag_name_taken" },
      );
      return tagView(first(tag ? [tag] : [], "tag_not_found"));
    });
  }

  /** Deleting a tag takes it off every ticket. */
  async deleteTag(user: Principal, id: string) {
    await this.run(async (tx) => {
      const tag = await this.tagRow(tx, user, id);
      await permit(tx, "tag", "delete", user.id, tag.workspace_id);
      await tx.delete(tags).where(eq(tags.id, id));
    });
  }

  async ticketTags(user: Principal, ticketId: string) {
    return await this.run(async (tx) => {
      const ticket = await this.ticketRow(tx, user, ticketId);
      const rows = await tx
        .select({ placed: ticketTags, tag: tags })
        .from(ticketTags)
        .innerJoin(tags, eq(tags.id, ticketTags.tag_id))
        .where(
          and(
            eq(ticketTags.ticket_id, ticket.id),
            granted(tx, "ticketTag", "read", user.id, ticketTags.workspace_id),
          ),
        )
        .orderBy(asc(ticketTags.created_at), asc(ticketTags.id))
        .limit(100);
      return rows.map((row) => ticketTagView(row.placed, row.tag));
    });
  }

  async tagTicket(
    user: Principal,
    ticketId: string,
    input: { tag_id: string },
  ) {
    return await this.run(async (tx) => {
      const ticket = await this.ticketRow(tx, user, ticketId);
      const tag = await this.tagRow(tx, user, input.tag_id);
      await permit(tx, "ticketTag", "create", user.id, ticket.workspace_id);
      sameWorkspace(ticket.workspace_id, tag.workspace_id);
      const [placed] = await unique(
        tx
          .insert(ticketTags)
          .values({
            id: this.id(),
            workspace_id: ticket.workspace_id,
            ticket_id: ticket.id,
            tag_id: tag.id,
            created_at: this.now(),
          })
          .returning(),
        { ticket_tag_once_per_ticket_key: "already_tagged" },
      );
      return ticketTagView(
        first(placed ? [placed] : [], "internal_fault"),
        tag,
      );
    });
  }

  async untagTicket(user: Principal, ticketId: string, tagId: string) {
    await this.run(async (tx) => {
      const ticket = await this.ticketRow(tx, user, ticketId);
      const rows = await tx
        .select()
        .from(ticketTags)
        .where(
          and(
            eq(ticketTags.ticket_id, ticket.id),
            eq(ticketTags.tag_id, tagId),
            granted(tx, "ticketTag", "read", user.id, ticketTags.workspace_id),
          ),
        );
      const placed = first(rows, "ticket_tag_not_found");
      await permit(tx, "ticketTag", "delete", user.id, placed.workspace_id);
      await tx.delete(ticketTags).where(eq(ticketTags.id, placed.id));
    });
  }

  // Macros ---------------------------------------------------------------

  async macros(user: Principal, workspace: string) {
    return await this.run(async (tx) => {
      const rows = await tx
        .select()
        .from(macros)
        .where(
          and(
            eq(macros.workspace_id, workspace),
            granted(tx, "macro", "read", user.id, macros.workspace_id),
          ),
        )
        .orderBy(sql`${macros.title} COLLATE "C"`, asc(macros.id))
        .limit(200);
      return rows.map(macroView);
    });
  }

  async createMacro(
    user: Principal,
    workspace: string,
    input: { title: string; body: string },
  ) {
    return await this.run(async (tx) => {
      await this.creating(tx, "macro", user, workspace);
      const now = this.now();
      const [saved] = await tx
        .insert(macros)
        .values({
          id: this.id(),
          workspace_id: workspace,
          title: input.title,
          body: input.body,
          created_at: now,
          updated_at: now,
        })
        .returning();
      return macroView(first(saved ? [saved] : [], "internal_fault"));
    });
  }

  async macro(user: Principal, id: string) {
    return await this.run(async (tx) =>
      macroView(await this.macroRow(tx, user, id)),
    );
  }

  async editMacro(
    user: Principal,
    id: string,
    input: Patch<{ title: string; body: string }>,
  ) {
    return await this.run(async (tx) => {
      const saved = await this.macroRow(tx, user, id);
      await permit(tx, "macro", "update", user.id, saved.workspace_id);
      const values = changes(input);
      if (unchanged(values)) return macroView(saved);
      const [changed] = await tx
        .update(macros)
        .set({ ...values, updated_at: this.now() })
        .where(eq(macros.id, id))
        .returning();
      return macroView(first(changed ? [changed] : [], "macro_not_found"));
    });
  }

  async deleteMacro(user: Principal, id: string) {
    await this.run(async (tx) => {
      const saved = await this.macroRow(tx, user, id);
      await permit(tx, "macro", "delete", user.id, saved.workspace_id);
      await tx.delete(macros).where(eq(macros.id, id));
    });
  }

  // Background work, as the system: every workspace, no caller ------------

  /** Open tickets nobody answered by their first-response deadline get
   * `breached_at`; at most 500 a run, the most overdue first. */
  async flagBreaches(): Promise<number> {
    return await this.run(async (tx) => {
      const now = this.now();
      const due = await tx
        .select({ id: tickets.id })
        .from(tickets)
        .where(
          and(
            isNull(tickets.deleted_at),
            eq(tickets.status, "open"),
            isNull(tickets.first_response_at),
            isNull(tickets.breached_at),
            lt(tickets.respond_by, now),
          ),
        )
        .orderBy(asc(tickets.respond_by), asc(tickets.id))
        .limit(500);
      if (!due.length) return 0;
      await tx
        .update(tickets)
        .set({ breached_at: now, updated_at: now })
        .where(
          inArray(
            tickets.id,
            due.map((row) => row.id),
          ),
        );
      return due.length;
    });
  }

  /** Sign-in sessions that have ended are deleted. */
  async forgetEndedSessions(): Promise<number> {
    return await this.run(async (tx) => {
      const ended = await tx
        .delete(sessions)
        .where(lte(sessions.ends_at, this.now()))
        .returning({ id: sessions.id });
      return ended.length;
    });
  }

  /** Tickets in the bin longer than the retention are deleted for good,
   * with their replies, notes and tags; 20 batches of 500 at most a run. */
  async purgeBinned(): Promise<number> {
    const before = new Date(
      this.clock() - this.config.deletedTicketRetentionMs,
    );
    let purged = 0;
    for (let batch = 0; batch < 20; batch++) {
      const deleted = await this.run(async (tx) => {
        const old = tx
          .select({ id: tickets.id })
          .from(tickets)
          .where(lt(tickets.deleted_at, before))
          .orderBy(asc(tickets.deleted_at), asc(tickets.id))
          .limit(500);
        return await tx
          .delete(tickets)
          .where(inArray(tickets.id, old))
          .returning({ id: tickets.id });
      });
      purged += deleted.length;
      if (deleted.length < 500) break;
    }
    return purged;
  }
}
