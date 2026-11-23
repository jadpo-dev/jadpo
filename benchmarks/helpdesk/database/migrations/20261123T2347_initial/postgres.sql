-- 20261123T2347_initial: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
CREATE TABLE "customer" (
    "id" uuid NOT NULL,
    "workspace_id" uuid NOT NULL,
    "name" text NOT NULL,
    "email" text NOT NULL,
    "phone" text,
    "created_at" timestamptz(3) NOT NULL,
    "updated_at" timestamptz(3) NOT NULL,
    CONSTRAINT "customer_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "jadpo_circuit_breakers" (
    "connection" text NOT NULL,
    "state" text NOT NULL,
    "failures" integer NOT NULL,
    "opened_at" timestamptz(3),
    "pauses" integer NOT NULL,
    "probe_at" timestamptz(3),
    "probe_id" uuid,
    "probe_until" timestamptz(3),
    "alerted_at" timestamptz(3),
    CONSTRAINT "jadpo_circuit_breakers_pkey" PRIMARY KEY ("connection")
);
CREATE TABLE "jadpo_deliveries" (
    "id" uuid NOT NULL,
    "outbox_id" uuid NOT NULL,
    "subscriber" text NOT NULL,
    "status" text NOT NULL,
    "attempts" integer NOT NULL,
    "next_attempt_at" timestamptz(3),
    "last_failure" jsonb,
    "last_tried_at" timestamptz(3),
    "retried_at" timestamptz(3),
    "held_for" text,
    "held_since" timestamptz(3),
    "held_ms" bigint,
    CONSTRAINT "jadpo_deliveries_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "jadpo_handled_deliveries" (
    "id" uuid NOT NULL,
    "handled_at" timestamptz(3) NOT NULL,
    CONSTRAINT "jadpo_handled_deliveries_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "jadpo_housekeeping" (
    "slot" integer NOT NULL,
    "lease_id" uuid,
    "lease_until" timestamptz(3),
    "last_cleaned_at" timestamptz(3),
    CONSTRAINT "jadpo_housekeeping_pkey" PRIMARY KEY ("slot")
);
CREATE TABLE "jadpo_idempotency_keys" (
    "scope" text NOT NULL,
    "key" text NOT NULL,
    "request_digest" text NOT NULL,
    "status" smallint,
    "response" jsonb,
    "stored_at" timestamptz(3) NOT NULL,
    "kept_until" timestamptz(3) NOT NULL,
    CONSTRAINT "jadpo_idempotency_keys_pkey" PRIMARY KEY ("scope", "key")
);
CREATE TABLE "jadpo_operator_actions" (
    "id" uuid NOT NULL,
    "acted_at" timestamptz(3) NOT NULL,
    "action" text NOT NULL,
    "kind" text NOT NULL,
    "item" uuid NOT NULL,
    "target" text NOT NULL,
    "event" text,
    "was" text NOT NULL,
    "operator" text NOT NULL,
    "role" text NOT NULL,
    CONSTRAINT "jadpo_operator_actions_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "jadpo_outbox" (
    "id" uuid NOT NULL,
    "event" text NOT NULL,
    "payload" jsonb NOT NULL,
    "emitted_at" timestamptz(3) NOT NULL,
    "traceparent" text,
    "encoding" smallint,
    "depth" integer,
    CONSTRAINT "jadpo_outbox_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "jadpo_schedule_runs" (
    "id" uuid NOT NULL,
    "schedule" text NOT NULL,
    "due_slot" timestamptz(3) NOT NULL,
    "started_at" timestamptz(3) NOT NULL,
    "ended_at" timestamptz(3),
    "outcome" text NOT NULL,
    "reason" text,
    "failure" jsonb,
    "events" integer,
    "traceparent" text,
    "items" integer,
    CONSTRAINT "jadpo_schedule_runs_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "jadpo_schedules" (
    "schedule" text NOT NULL,
    "every_ms" bigint NOT NULL,
    "next_run_at" timestamptz(3) NOT NULL,
    "paused" boolean NOT NULL,
    "lease_until" timestamptz(3),
    "run_id" uuid,
    "last_tick_at" timestamptz(3),
    "last_succeeded_at" timestamptz(3),
    "run_requested" uuid,
    "alerts" jsonb,
    "lease_holder" text,
    CONSTRAINT "jadpo_schedules_pkey" PRIMARY KEY ("schedule")
);
CREATE TABLE "jadpo_sessions" (
    "id" uuid NOT NULL,
    "principal_id" uuid NOT NULL,
    "verifier" text NOT NULL,
    "csrf_verifier" text,
    "started_at" timestamptz(3) NOT NULL,
    "ends_at" timestamptz(3) NOT NULL,
    "sign_in_method" text,
    "strength" text,
    CONSTRAINT "jadpo_sessions_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "macro" (
    "id" uuid NOT NULL,
    "workspace_id" uuid NOT NULL,
    "title" text NOT NULL,
    "body" text NOT NULL,
    "created_at" timestamptz(3) NOT NULL,
    "updated_at" timestamptz(3) NOT NULL,
    CONSTRAINT "macro_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "membership" (
    "id" uuid NOT NULL,
    "workspace_id" uuid NOT NULL,
    "user_id" uuid NOT NULL,
    "role" text NOT NULL,
    "display_name" text NOT NULL,
    "created_at" timestamptz(3) NOT NULL,
    "updated_at" timestamptz(3) NOT NULL,
    CONSTRAINT "membership_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "note" (
    "id" uuid NOT NULL,
    "workspace_id" uuid NOT NULL,
    "ticket_id" uuid NOT NULL,
    "author_id" uuid,
    "body" text NOT NULL,
    "created_at" timestamptz(3) NOT NULL,
    "updated_at" timestamptz(3) NOT NULL,
    CONSTRAINT "note_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "reply" (
    "id" uuid NOT NULL,
    "workspace_id" uuid NOT NULL,
    "ticket_id" uuid NOT NULL,
    "author_id" uuid,
    "body" text NOT NULL,
    "created_at" timestamptz(3) NOT NULL,
    "updated_at" timestamptz(3) NOT NULL,
    CONSTRAINT "reply_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "tag" (
    "id" uuid NOT NULL,
    "workspace_id" uuid NOT NULL,
    "name" text NOT NULL,
    "created_at" timestamptz(3) NOT NULL,
    CONSTRAINT "tag_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "ticket" (
    "id" uuid NOT NULL,
    "workspace_id" uuid NOT NULL,
    "customer_id" uuid,
    "assignee_id" uuid,
    "subject" text NOT NULL,
    "status" text NOT NULL,
    "priority" text NOT NULL,
    "respond_by" timestamptz(3) NOT NULL,
    "first_response_at" timestamptz(3),
    "breached_at" timestamptz(3),
    "solved_at" timestamptz(3),
    "deleted_at" timestamptz(3),
    "created_at" timestamptz(3) NOT NULL,
    "updated_at" timestamptz(3) NOT NULL,
    CONSTRAINT "ticket_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "ticket_tag" (
    "id" uuid NOT NULL,
    "workspace_id" uuid NOT NULL,
    "ticket_id" uuid NOT NULL,
    "tag_id" uuid NOT NULL,
    "created_at" timestamptz(3) NOT NULL,
    CONSTRAINT "ticket_tag_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "user" (
    "id" uuid NOT NULL,
    "authentication_subject" text NOT NULL,
    "status" text NOT NULL,
    "email" text NOT NULL,
    "created_at" timestamptz(3) NOT NULL,
    CONSTRAINT "user_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "workspace" (
    "id" uuid NOT NULL,
    "name" text NOT NULL,
    "created_at" timestamptz(3) NOT NULL,
    "updated_at" timestamptz(3) NOT NULL,
    CONSTRAINT "workspace_pkey" PRIMARY KEY ("id")
);
ALTER TABLE "customer" ADD CONSTRAINT "customer_email_key" UNIQUE ("email", "workspace_id");
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_subscriber_key" UNIQUE ("outbox_id", "subscriber");
ALTER TABLE "jadpo_sessions" ADD CONSTRAINT "jadpo_sessions_verifier_key" UNIQUE ("verifier");
ALTER TABLE "membership" ADD CONSTRAINT "membership_one_per_person_key" UNIQUE ("workspace_id", "user_id");
ALTER TABLE "tag" ADD CONSTRAINT "tag_name_key" UNIQUE ("name", "workspace_id");
ALTER TABLE "ticket_tag" ADD CONSTRAINT "ticket_tag_once_per_ticket_key" UNIQUE ("ticket_id", "tag_id", "workspace_id");
ALTER TABLE "user" ADD CONSTRAINT "user_authentication_subject_key" UNIQUE ("authentication_subject");
ALTER TABLE "customer" ADD CONSTRAINT "customer_workspace_id_fkey" FOREIGN KEY ("workspace_id") REFERENCES "workspace" ("id") ON DELETE CASCADE;
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_fkey" FOREIGN KEY ("outbox_id") REFERENCES "jadpo_outbox" ("id") ON DELETE CASCADE;
ALTER TABLE "macro" ADD CONSTRAINT "macro_workspace_id_fkey" FOREIGN KEY ("workspace_id") REFERENCES "workspace" ("id") ON DELETE CASCADE;
ALTER TABLE "membership" ADD CONSTRAINT "membership_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "user" ("id") ON DELETE CASCADE;
ALTER TABLE "membership" ADD CONSTRAINT "membership_workspace_id_fkey" FOREIGN KEY ("workspace_id") REFERENCES "workspace" ("id") ON DELETE CASCADE;
ALTER TABLE "note" ADD CONSTRAINT "note_author_id_fkey" FOREIGN KEY ("author_id") REFERENCES "user" ("id") ON DELETE SET NULL;
ALTER TABLE "note" ADD CONSTRAINT "note_ticket_id_fkey" FOREIGN KEY ("ticket_id") REFERENCES "ticket" ("id") ON DELETE CASCADE;
ALTER TABLE "note" ADD CONSTRAINT "note_workspace_id_fkey" FOREIGN KEY ("workspace_id") REFERENCES "workspace" ("id") ON DELETE CASCADE;
ALTER TABLE "reply" ADD CONSTRAINT "reply_author_id_fkey" FOREIGN KEY ("author_id") REFERENCES "user" ("id") ON DELETE SET NULL;
ALTER TABLE "reply" ADD CONSTRAINT "reply_ticket_id_fkey" FOREIGN KEY ("ticket_id") REFERENCES "ticket" ("id") ON DELETE CASCADE;
ALTER TABLE "reply" ADD CONSTRAINT "reply_workspace_id_fkey" FOREIGN KEY ("workspace_id") REFERENCES "workspace" ("id") ON DELETE CASCADE;
ALTER TABLE "tag" ADD CONSTRAINT "tag_workspace_id_fkey" FOREIGN KEY ("workspace_id") REFERENCES "workspace" ("id") ON DELETE CASCADE;
ALTER TABLE "ticket" ADD CONSTRAINT "ticket_assignee_id_fkey" FOREIGN KEY ("assignee_id") REFERENCES "membership" ("id") ON DELETE SET NULL;
ALTER TABLE "ticket" ADD CONSTRAINT "ticket_customer_id_fkey" FOREIGN KEY ("customer_id") REFERENCES "customer" ("id") ON DELETE SET NULL;
ALTER TABLE "ticket" ADD CONSTRAINT "ticket_workspace_id_fkey" FOREIGN KEY ("workspace_id") REFERENCES "workspace" ("id") ON DELETE CASCADE;
ALTER TABLE "ticket_tag" ADD CONSTRAINT "ticket_tag_tag_id_fkey" FOREIGN KEY ("tag_id") REFERENCES "tag" ("id") ON DELETE CASCADE;
ALTER TABLE "ticket_tag" ADD CONSTRAINT "ticket_tag_ticket_id_fkey" FOREIGN KEY ("ticket_id") REFERENCES "ticket" ("id") ON DELETE CASCADE;
ALTER TABLE "ticket_tag" ADD CONSTRAINT "ticket_tag_workspace_id_fkey" FOREIGN KEY ("workspace_id") REFERENCES "workspace" ("id") ON DELETE CASCADE;
CREATE INDEX "customer_workspace_id_created_at_id_idx" ON "customer" ("workspace_id", "created_at", "id");
CREATE INDEX "jadpo_deliveries_status_next_attempt_at_idx" ON "jadpo_deliveries" ("status", "next_attempt_at");
CREATE INDEX "jadpo_idempotency_keys_kept_until_idx" ON "jadpo_idempotency_keys" ("kept_until");
CREATE INDEX "jadpo_operator_actions_item_idx" ON "jadpo_operator_actions" ("item");
CREATE INDEX "jadpo_outbox_emitted_at_idx" ON "jadpo_outbox" ("emitted_at");
CREATE INDEX "jadpo_schedule_runs_schedule_started_at_idx" ON "jadpo_schedule_runs" ("schedule", "started_at");
CREATE INDEX "jadpo_schedule_runs_started_at_idx" ON "jadpo_schedule_runs" ("started_at");
CREATE INDEX "jadpo_sessions_ends_at_idx" ON "jadpo_sessions" ("ends_at");
CREATE INDEX "jadpo_sessions_principal_id_idx" ON "jadpo_sessions" ("principal_id");
CREATE INDEX "macro_workspace_id_title_id_sorted_idx" ON "macro" ("workspace_id", "title" COLLATE "C", "id");
CREATE INDEX "membership_user_id_idx" ON "membership" ("user_id");
CREATE INDEX "membership_workspace_id_created_at_id_idx" ON "membership" ("workspace_id", "created_at", "id");
CREATE INDEX "membership_workspace_id_role_id_idx" ON "membership" ("workspace_id", "role", "id");
CREATE INDEX "note_author_id_idx" ON "note" ("author_id");
CREATE INDEX "note_ticket_id_created_at_id_idx" ON "note" ("ticket_id", "created_at", "id");
CREATE INDEX "note_workspace_id_idx" ON "note" ("workspace_id");
CREATE INDEX "reply_author_id_idx" ON "reply" ("author_id");
CREATE INDEX "reply_ticket_id_created_at_id_idx" ON "reply" ("ticket_id", "created_at", "id");
CREATE INDEX "reply_workspace_id_idx" ON "reply" ("workspace_id");
CREATE INDEX "tag_workspace_id_name_id_sorted_idx" ON "tag" ("workspace_id", "name" COLLATE "C", "id");
CREATE INDEX "ticket_assignee_id_idx" ON "ticket" ("assignee_id");
CREATE INDEX "ticket_customer_id_created_at_id_idx" ON "ticket" ("customer_id", "created_at", "id") WHERE "deleted_at" IS NULL;
CREATE INDEX "ticket_customer_id_idx" ON "ticket" ("customer_id");
CREATE INDEX "ticket_status_respond_by_id_idx" ON "ticket" ("status", "respond_by", "id") WHERE "deleted_at" IS NULL;
CREATE INDEX "ticket_workspace_id_created_at_id_idx" ON "ticket" ("workspace_id", "created_at", "id") WHERE "deleted_at" IS NULL;
CREATE INDEX "ticket_workspace_id_idx" ON "ticket" ("workspace_id");
CREATE INDEX "ticket_tag_tag_id_idx" ON "ticket_tag" ("tag_id");
CREATE INDEX "ticket_tag_ticket_id_created_at_id_idx" ON "ticket_tag" ("ticket_id", "created_at", "id");
CREATE INDEX "ticket_tag_workspace_id_idx" ON "ticket_tag" ("workspace_id");
CREATE INDEX "workspace_created_at_id_idx" ON "workspace" ("created_at", "id");
