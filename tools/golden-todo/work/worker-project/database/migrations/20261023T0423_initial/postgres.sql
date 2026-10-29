-- 20261023T0423_initial: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
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
    CONSTRAINT "jadpo_schedules_pkey" PRIMARY KEY ("schedule")
);
CREATE TABLE "jadpo_service_keys" (
    "id" uuid NOT NULL,
    "principal_id" uuid NOT NULL,
    "verifier" text NOT NULL,
    "started_at" timestamptz(3) NOT NULL,
    "ends_at" timestamptz(3) NOT NULL,
    CONSTRAINT "jadpo_service_keys_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "jadpo_sessions" (
    "id" uuid NOT NULL,
    "principal_id" uuid NOT NULL,
    "verifier" text NOT NULL,
    "csrf_verifier" text,
    "started_at" timestamptz(3) NOT NULL,
    "ends_at" timestamptz(3) NOT NULL,
    CONSTRAINT "jadpo_sessions_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "service" (
    "id" uuid NOT NULL,
    "owner_id" uuid NOT NULL,
    "name" text NOT NULL,
    "status" text NOT NULL,
    CONSTRAINT "service_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "todo" (
    "id" uuid NOT NULL,
    "owner_id" uuid NOT NULL,
    "title" text NOT NULL,
    "status" text NOT NULL,
    "due_at" timestamptz(3),
    "reminder_revision" bigint NOT NULL,
    "reminder_queued_at" timestamptz(3),
    "reminder_sent_at" timestamptz(3),
    "deleted_at" timestamptz(3),
    "created_at" timestamptz(3) NOT NULL,
    "updated_at" timestamptz(3) NOT NULL,
    CONSTRAINT "todo_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "user" (
    "id" uuid NOT NULL,
    "authentication_subject" text NOT NULL,
    "status" text NOT NULL,
    "email" text NOT NULL,
    CONSTRAINT "user_pkey" PRIMARY KEY ("id")
);
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_subscriber_key" UNIQUE ("outbox_id", "subscriber");
ALTER TABLE "jadpo_service_keys" ADD CONSTRAINT "jadpo_service_keys_verifier_key" UNIQUE ("verifier");
ALTER TABLE "jadpo_sessions" ADD CONSTRAINT "jadpo_sessions_verifier_key" UNIQUE ("verifier");
ALTER TABLE "service" ADD CONSTRAINT "service_name_key" UNIQUE ("name");
ALTER TABLE "user" ADD CONSTRAINT "user_authentication_subject_key" UNIQUE ("authentication_subject");
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_fkey" FOREIGN KEY ("outbox_id") REFERENCES "jadpo_outbox" ("id") ON DELETE CASCADE;
ALTER TABLE "service" ADD CONSTRAINT "service_owner_id_fkey" FOREIGN KEY ("owner_id") REFERENCES "user" ("id") ON DELETE RESTRICT;
ALTER TABLE "todo" ADD CONSTRAINT "todo_owner_id_fkey" FOREIGN KEY ("owner_id") REFERENCES "user" ("id") ON DELETE RESTRICT;
CREATE INDEX "jadpo_deliveries_status_next_attempt_at_idx" ON "jadpo_deliveries" ("status", "next_attempt_at");
CREATE INDEX "jadpo_idempotency_keys_kept_until_idx" ON "jadpo_idempotency_keys" ("kept_until");
CREATE INDEX "jadpo_operator_actions_item_idx" ON "jadpo_operator_actions" ("item");
CREATE INDEX "jadpo_outbox_emitted_at_idx" ON "jadpo_outbox" ("emitted_at");
CREATE INDEX "jadpo_schedule_runs_schedule_started_at_idx" ON "jadpo_schedule_runs" ("schedule", "started_at");
CREATE INDEX "jadpo_schedule_runs_started_at_idx" ON "jadpo_schedule_runs" ("started_at");
CREATE INDEX "jadpo_service_keys_principal_id_idx" ON "jadpo_service_keys" ("principal_id");
CREATE INDEX "jadpo_sessions_ends_at_idx" ON "jadpo_sessions" ("ends_at");
CREATE INDEX "jadpo_sessions_principal_id_idx" ON "jadpo_sessions" ("principal_id");
CREATE INDEX "service_owner_id_idx" ON "service" ("owner_id");
CREATE INDEX "todo_created_at_id_idx" ON "todo" ("created_at", "id") WHERE "deleted_at" IS NULL;
CREATE INDEX "todo_owner_id_created_at_id_idx" ON "todo" ("owner_id", "created_at", "id") WHERE "deleted_at" IS NULL;
CREATE INDEX "todo_owner_id_idx" ON "todo" ("owner_id");
CREATE INDEX "todo_status_created_at_id_idx" ON "todo" ("status", "created_at", "id") WHERE "deleted_at" IS NULL;
