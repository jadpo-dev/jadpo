import {
  pgTable,
  uuid,
  text,
  timestamp,
  bigint,
  integer,
  jsonb,
  index,
  primaryKey,
} from "drizzle-orm/pg-core";
const instant = (name: string) =>
  timestamp(name, { withTimezone: true, mode: "string" });
export const users = pgTable("user", {
  id: uuid().primaryKey(),
  authentication_subject: text().notNull().unique(),
  status: text().notNull(),
  email: text().notNull(),
});
export const services = pgTable("service", {
  id: uuid().primaryKey(),
  owner_id: uuid()
    .notNull()
    .references(() => users.id, { onDelete: "restrict" }),
  name: text().notNull().unique(),
  status: text().notNull(),
});
export const todos = pgTable(
  "todo",
  {
    id: uuid().primaryKey(),
    owner_id: uuid()
      .notNull()
      .references(() => users.id, { onDelete: "restrict" }),
    title: text().notNull(),
    status: text().notNull(),
    due_at: instant("due_at"),
    reminder_sent_at: instant("reminder_sent_at"),
    reminder_queued_at: instant("reminder_queued_at"),
    reminder_revision: bigint({ mode: "bigint" }).notNull(),
    deleted_at: instant("deleted_at"),
    created_at: instant("created_at").notNull(),
    updated_at: instant("updated_at").notNull(),
  },
  (table) => [
    index("todo_owner_page").on(table.owner_id, table.created_at, table.id),
  ],
);
export const sessions = pgTable("jadpo_sessions", {
  id: uuid().primaryKey(),
  principal_id: uuid()
    .notNull()
    .references(() => users.id),
  verifier: text().notNull().unique(),
  csrf_verifier: text(),
  started_at: instant("started_at").notNull(),
  ends_at: instant("ends_at").notNull(),
});
export const serviceKeys = pgTable("jadpo_service_keys", {
  id: uuid().primaryKey(),
  principal_id: uuid()
    .notNull()
    .references(() => services.id),
  verifier: text().notNull().unique(),
  started_at: instant("started_at").notNull(),
  ends_at: instant("ends_at").notNull(),
});
export const deliveries = pgTable("jadpo_deliveries", {
  id: uuid().primaryKey(),
  subscriber: text().notNull(),
  payload: jsonb().notNull(),
  status: text().notNull(),
  lease_token: uuid(),
  lease_until: instant("lease_until"),
  attempts: integer().notNull().default(0),
  first_attempt_at: instant("first_attempt_at"),
  next_attempt_at: instant("next_attempt_at").notNull(),
  provider_key: text().notNull().unique(),
  last_failure: jsonb(),
  created_at: instant("created_at").notNull(),
});
export const scheduleRuns = pgTable("jadpo_schedule_runs", {
  id: uuid().primaryKey(),
  schedule: text().notNull(),
  started_at: instant("started_at").notNull(),
  outcome: text().notNull(),
  items: integer().notNull(),
});
export const scheduleClaims = pgTable("todo_schedule_claims", {
  name: text().primaryKey(),
  bucket: bigint({ mode: "number" }).notNull(),
});

export const idempotency = pgTable(
  "todo_request_receipts",
  {
    caller: text().notNull(),
    key: text().notNull(),
    fingerprint: text().notNull(),
    token: uuid().notNull(),
    lease_until: instant("lease_until").notNull(),
    expires_at: instant("expires_at").notNull(),
    response_status: integer(),
    response_body: text(),
  },
  (table) => [primaryKey({ columns: [table.caller, table.key] })],
);

export const providerBreakers = pgTable("todo_provider_breakers", {
  name: text().primaryKey(),
  failures: integer().notNull().default(0),
  open_until: instant("open_until"),
  backoff_ms: integer().notNull().default(60000),
  probe_token: uuid(),
  probe_until: instant("probe_until"),
});
