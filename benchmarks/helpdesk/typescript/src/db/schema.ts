// The helpdesk's tables. Names, types, keys and ON DELETE actions are the
// Jadpo app's migration (benchmarks/helpdesk/database/migrations), so the
// same SQL fixtures load into either twin.
import { sql } from "drizzle-orm";
import {
  foreignKey,
  index,
  pgTable,
  text,
  timestamp,
  unique,
  uuid,
} from "drizzle-orm/pg-core";

export const ROLES = ["owner", "admin", "agent", "viewer"] as const;
export const TICKET_STATUSES = ["open", "pending", "solved"] as const;
export const PRIORITIES = ["low", "normal", "high", "urgent"] as const;
export const USER_STATUSES = ["active", "disabled"] as const;
export type Role = (typeof ROLES)[number];
export type TicketStatus = (typeof TICKET_STATUSES)[number];
export type Priority = (typeof PRIORITIES)[number];

const instant = (name: string) =>
  timestamp(name, { withTimezone: true, precision: 3, mode: "date" });

export const users = pgTable(
  "user",
  {
    id: uuid("id").primaryKey(),
    authentication_subject: text("authentication_subject").notNull(),
    status: text("status", { enum: USER_STATUSES }).notNull(),
    email: text("email").notNull(),
    created_at: instant("created_at").notNull(),
  },
  (t) => [
    unique("user_authentication_subject_key").on(t.authentication_subject),
  ],
);

export const workspaces = pgTable(
  "workspace",
  {
    id: uuid("id").primaryKey(),
    name: text("name").notNull(),
    created_at: instant("created_at").notNull(),
    updated_at: instant("updated_at").notNull(),
  },
  (t) => [index("workspace_created_at_id_idx").on(t.created_at, t.id)],
);

export const memberships = pgTable(
  "membership",
  {
    id: uuid("id").primaryKey(),
    workspace_id: uuid("workspace_id").notNull(),
    user_id: uuid("user_id").notNull(),
    role: text("role", { enum: ROLES }).notNull(),
    display_name: text("display_name").notNull(),
    created_at: instant("created_at").notNull(),
    updated_at: instant("updated_at").notNull(),
  },
  (t) => [
    unique("membership_one_per_person_key").on(t.workspace_id, t.user_id),
    foreignKey({
      name: "membership_workspace_id_fkey",
      columns: [t.workspace_id],
      foreignColumns: [workspaces.id],
    }).onDelete("cascade"),
    foreignKey({
      name: "membership_user_id_fkey",
      columns: [t.user_id],
      foreignColumns: [users.id],
    }).onDelete("cascade"),
    index("membership_user_id_idx").on(t.user_id),
    index("membership_workspace_id_created_at_id_idx").on(
      t.workspace_id,
      t.created_at,
      t.id,
    ),
    index("membership_workspace_id_role_id_idx").on(
      t.workspace_id,
      t.role,
      t.id,
    ),
  ],
);

export const customers = pgTable(
  "customer",
  {
    id: uuid("id").primaryKey(),
    workspace_id: uuid("workspace_id").notNull(),
    name: text("name").notNull(),
    email: text("email").notNull(),
    phone: text("phone"),
    created_at: instant("created_at").notNull(),
    updated_at: instant("updated_at").notNull(),
  },
  (t) => [
    unique("customer_email_key").on(t.email, t.workspace_id),
    foreignKey({
      name: "customer_workspace_id_fkey",
      columns: [t.workspace_id],
      foreignColumns: [workspaces.id],
    }).onDelete("cascade"),
    index("customer_workspace_id_created_at_id_idx").on(
      t.workspace_id,
      t.created_at,
      t.id,
    ),
  ],
);

export const tickets = pgTable(
  "ticket",
  {
    id: uuid("id").primaryKey(),
    workspace_id: uuid("workspace_id").notNull(),
    customer_id: uuid("customer_id"),
    assignee_id: uuid("assignee_id"),
    subject: text("subject").notNull(),
    status: text("status", { enum: TICKET_STATUSES }).notNull(),
    priority: text("priority", { enum: PRIORITIES }).notNull(),
    respond_by: instant("respond_by").notNull(),
    first_response_at: instant("first_response_at"),
    breached_at: instant("breached_at"),
    solved_at: instant("solved_at"),
    deleted_at: instant("deleted_at"),
    created_at: instant("created_at").notNull(),
    updated_at: instant("updated_at").notNull(),
  },
  (t) => [
    foreignKey({
      name: "ticket_workspace_id_fkey",
      columns: [t.workspace_id],
      foreignColumns: [workspaces.id],
    }).onDelete("cascade"),
    foreignKey({
      name: "ticket_customer_id_fkey",
      columns: [t.customer_id],
      foreignColumns: [customers.id],
    }).onDelete("set null"),
    foreignKey({
      name: "ticket_assignee_id_fkey",
      columns: [t.assignee_id],
      foreignColumns: [memberships.id],
    }).onDelete("set null"),
    index("ticket_assignee_id_idx").on(t.assignee_id),
    index("ticket_customer_id_idx").on(t.customer_id),
    index("ticket_workspace_id_idx").on(t.workspace_id),
    index("ticket_customer_id_created_at_id_idx")
      .on(t.customer_id, t.created_at, t.id)
      .where(sql`"deleted_at" IS NULL`),
    index("ticket_status_respond_by_id_idx")
      .on(t.status, t.respond_by, t.id)
      .where(sql`"deleted_at" IS NULL`),
    index("ticket_workspace_id_created_at_id_idx")
      .on(t.workspace_id, t.created_at, t.id)
      .where(sql`"deleted_at" IS NULL`),
  ],
);

const message = (table: "reply" | "note") =>
  pgTable(
    table,
    {
      id: uuid("id").primaryKey(),
      workspace_id: uuid("workspace_id").notNull(),
      ticket_id: uuid("ticket_id").notNull(),
      author_id: uuid("author_id"),
      body: text("body").notNull(),
      created_at: instant("created_at").notNull(),
      updated_at: instant("updated_at").notNull(),
    },
    (t) => [
      foreignKey({
        name: `${table}_workspace_id_fkey`,
        columns: [t.workspace_id],
        foreignColumns: [workspaces.id],
      }).onDelete("cascade"),
      foreignKey({
        name: `${table}_ticket_id_fkey`,
        columns: [t.ticket_id],
        foreignColumns: [tickets.id],
      }).onDelete("cascade"),
      foreignKey({
        name: `${table}_author_id_fkey`,
        columns: [t.author_id],
        foreignColumns: [users.id],
      }).onDelete("set null"),
      index(`${table}_author_id_idx`).on(t.author_id),
      index(`${table}_workspace_id_idx`).on(t.workspace_id),
      index(`${table}_ticket_id_created_at_id_idx`).on(
        t.ticket_id,
        t.created_at,
        t.id,
      ),
    ],
  );
export const replies = message("reply");
export const notes = message("note");

export const tags = pgTable(
  "tag",
  {
    id: uuid("id").primaryKey(),
    workspace_id: uuid("workspace_id").notNull(),
    name: text("name").notNull(),
    created_at: instant("created_at").notNull(),
  },
  (t) => [
    unique("tag_name_key").on(t.name, t.workspace_id),
    foreignKey({
      name: "tag_workspace_id_fkey",
      columns: [t.workspace_id],
      foreignColumns: [workspaces.id],
    }).onDelete("cascade"),
    index("tag_workspace_id_name_id_sorted_idx").on(
      t.workspace_id,
      sql`"name" COLLATE "C"`,
      t.id,
    ),
  ],
);

export const ticketTags = pgTable(
  "ticket_tag",
  {
    id: uuid("id").primaryKey(),
    workspace_id: uuid("workspace_id").notNull(),
    ticket_id: uuid("ticket_id").notNull(),
    tag_id: uuid("tag_id").notNull(),
    created_at: instant("created_at").notNull(),
  },
  (t) => [
    unique("ticket_tag_once_per_ticket_key").on(
      t.ticket_id,
      t.tag_id,
      t.workspace_id,
    ),
    foreignKey({
      name: "ticket_tag_workspace_id_fkey",
      columns: [t.workspace_id],
      foreignColumns: [workspaces.id],
    }).onDelete("cascade"),
    foreignKey({
      name: "ticket_tag_ticket_id_fkey",
      columns: [t.ticket_id],
      foreignColumns: [tickets.id],
    }).onDelete("cascade"),
    foreignKey({
      name: "ticket_tag_tag_id_fkey",
      columns: [t.tag_id],
      foreignColumns: [tags.id],
    }).onDelete("cascade"),
    index("ticket_tag_tag_id_idx").on(t.tag_id),
    index("ticket_tag_workspace_id_idx").on(t.workspace_id),
    index("ticket_tag_ticket_id_created_at_id_idx").on(
      t.ticket_id,
      t.created_at,
      t.id,
    ),
  ],
);

export const macros = pgTable(
  "macro",
  {
    id: uuid("id").primaryKey(),
    workspace_id: uuid("workspace_id").notNull(),
    title: text("title").notNull(),
    body: text("body").notNull(),
    created_at: instant("created_at").notNull(),
    updated_at: instant("updated_at").notNull(),
  },
  (t) => [
    foreignKey({
      name: "macro_workspace_id_fkey",
      columns: [t.workspace_id],
      foreignColumns: [workspaces.id],
    }).onDelete("cascade"),
    index("macro_workspace_id_title_id_sorted_idx").on(
      t.workspace_id,
      sql`"title" COLLATE "C"`,
      t.id,
    ),
  ],
);

// Sign-in sessions, in the layout Jadpo's runtime keeps them: API tokens and
// browser sessions are issued by trusted infrastructure, never by this app.
export const sessions = pgTable(
  "jadpo_sessions",
  {
    id: uuid("id").primaryKey(),
    principal_id: uuid("principal_id").notNull(),
    verifier: text("verifier").notNull(),
    csrf_verifier: text("csrf_verifier"),
    started_at: instant("started_at").notNull(),
    ends_at: instant("ends_at").notNull(),
    sign_in_method: text("sign_in_method"),
    strength: text("strength"),
  },
  (t) => [
    unique("jadpo_sessions_verifier_key").on(t.verifier),
    index("jadpo_sessions_ends_at_idx").on(t.ends_at),
    index("jadpo_sessions_principal_id_idx").on(t.principal_id),
  ],
);
