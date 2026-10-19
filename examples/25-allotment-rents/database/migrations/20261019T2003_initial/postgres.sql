-- 20261019T2003_initial: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
CREATE TABLE "gardener" (
    "id" uuid NOT NULL,
    "email" text NOT NULL,
    "name" text NOT NULL,
    CONSTRAINT "gardener_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "invoice" (
    "id" uuid NOT NULL,
    "society_id" uuid NOT NULL,
    "plot_id" uuid NOT NULL,
    "round_id" uuid NOT NULL,
    "amount" integer NOT NULL,
    CONSTRAINT "invoice_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "jadpo_batch_cancels" (
    "run_id" uuid NOT NULL,
    "requested_at" timestamptz(3) NOT NULL,
    CONSTRAINT "jadpo_batch_cancels_pkey" PRIMARY KEY ("run_id")
);
CREATE TABLE "jadpo_batch_problems" (
    "run_id" uuid NOT NULL,
    "row" bigint NOT NULL,
    "column" text NOT NULL,
    "rule" text NOT NULL,
    CONSTRAINT "jadpo_batch_problems_pkey" PRIMARY KEY ("run_id", "row")
);
CREATE TABLE "jadpo_batch_runs" (
    "id" uuid NOT NULL,
    "batch" text NOT NULL,
    "shape" text NOT NULL,
    "key" text,
    "captured" jsonb NOT NULL,
    "state" text NOT NULL,
    "position" text,
    "read_all" boolean NOT NULL,
    "items_done" bigint NOT NULL,
    "problems" bigint NOT NULL,
    "failure" text,
    "lease_until" timestamptz(3),
    "started_at" timestamptz(3) NOT NULL,
    "stepped_at" timestamptz(3) NOT NULL,
    "finished_at" timestamptz(3),
    CONSTRAINT "jadpo_batch_runs_pkey" PRIMARY KEY ("id")
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
    CONSTRAINT "jadpo_schedules_pkey" PRIMARY KEY ("schedule")
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
CREATE TABLE "officer" (
    "id" uuid NOT NULL,
    "society_id" uuid NOT NULL,
    "gardener_id" uuid NOT NULL,
    "role" text NOT NULL,
    CONSTRAINT "officer_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "plot" (
    "id" uuid NOT NULL,
    "society_id" uuid NOT NULL,
    "number" smallint NOT NULL,
    "holder" text,
    "rent" integer NOT NULL,
    "rent_due_on" date,
    "reminded_on" date,
    CONSTRAINT "plot_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "reminder" (
    "id" uuid NOT NULL,
    "society_id" uuid NOT NULL,
    "plot_id" uuid NOT NULL,
    "sent_on" date NOT NULL,
    CONSTRAINT "reminder_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "rent_round" (
    "id" uuid NOT NULL,
    "society_id" uuid NOT NULL,
    "year" smallint NOT NULL,
    "invoices" jsonb,
    CONSTRAINT "rent_round_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "society" (
    "id" uuid NOT NULL,
    "name" text NOT NULL,
    CONSTRAINT "society_pkey" PRIMARY KEY ("id")
);
ALTER TABLE "gardener" ADD CONSTRAINT "gardener_email_key" UNIQUE ("email");
ALTER TABLE "jadpo_batch_runs" ADD CONSTRAINT "jadpo_batch_runs_batch_key_key" UNIQUE ("batch", "key");
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_subscriber_key" UNIQUE ("outbox_id", "subscriber");
ALTER TABLE "jadpo_sessions" ADD CONSTRAINT "jadpo_sessions_verifier_key" UNIQUE ("verifier");
ALTER TABLE "invoice" ADD CONSTRAINT "invoice_plot_id_fkey" FOREIGN KEY ("plot_id") REFERENCES "plot" ("id") ON DELETE CASCADE;
ALTER TABLE "invoice" ADD CONSTRAINT "invoice_round_id_fkey" FOREIGN KEY ("round_id") REFERENCES "rent_round" ("id") ON DELETE CASCADE;
ALTER TABLE "invoice" ADD CONSTRAINT "invoice_society_id_fkey" FOREIGN KEY ("society_id") REFERENCES "society" ("id") ON DELETE CASCADE;
ALTER TABLE "jadpo_batch_problems" ADD CONSTRAINT "jadpo_batch_problems_run_id_fkey" FOREIGN KEY ("run_id") REFERENCES "jadpo_batch_runs" ("id") ON DELETE CASCADE;
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_fkey" FOREIGN KEY ("outbox_id") REFERENCES "jadpo_outbox" ("id") ON DELETE CASCADE;
ALTER TABLE "officer" ADD CONSTRAINT "officer_gardener_id_fkey" FOREIGN KEY ("gardener_id") REFERENCES "gardener" ("id") ON DELETE CASCADE;
ALTER TABLE "officer" ADD CONSTRAINT "officer_society_id_fkey" FOREIGN KEY ("society_id") REFERENCES "society" ("id") ON DELETE CASCADE;
ALTER TABLE "plot" ADD CONSTRAINT "plot_society_id_fkey" FOREIGN KEY ("society_id") REFERENCES "society" ("id") ON DELETE CASCADE;
ALTER TABLE "reminder" ADD CONSTRAINT "reminder_plot_id_fkey" FOREIGN KEY ("plot_id") REFERENCES "plot" ("id") ON DELETE CASCADE;
ALTER TABLE "reminder" ADD CONSTRAINT "reminder_society_id_fkey" FOREIGN KEY ("society_id") REFERENCES "society" ("id") ON DELETE CASCADE;
ALTER TABLE "rent_round" ADD CONSTRAINT "rent_round_society_id_fkey" FOREIGN KEY ("society_id") REFERENCES "society" ("id") ON DELETE CASCADE;
CREATE INDEX "invoice_plot_id_idx" ON "invoice" ("plot_id");
CREATE INDEX "invoice_round_id_idx" ON "invoice" ("round_id");
CREATE INDEX "invoice_society_id_idx" ON "invoice" ("society_id");
CREATE INDEX "jadpo_batch_runs_state_lease_until_idx" ON "jadpo_batch_runs" ("state", "lease_until");
CREATE INDEX "jadpo_deliveries_status_next_attempt_at_idx" ON "jadpo_deliveries" ("status", "next_attempt_at");
CREATE INDEX "jadpo_idempotency_keys_kept_until_idx" ON "jadpo_idempotency_keys" ("kept_until");
CREATE INDEX "jadpo_outbox_emitted_at_idx" ON "jadpo_outbox" ("emitted_at");
CREATE INDEX "jadpo_schedule_runs_schedule_started_at_idx" ON "jadpo_schedule_runs" ("schedule", "started_at");
CREATE INDEX "jadpo_schedule_runs_started_at_idx" ON "jadpo_schedule_runs" ("started_at");
CREATE INDEX "jadpo_sessions_ends_at_idx" ON "jadpo_sessions" ("ends_at");
CREATE INDEX "jadpo_sessions_principal_id_idx" ON "jadpo_sessions" ("principal_id");
CREATE INDEX "officer_gardener_id_idx" ON "officer" ("gardener_id");
CREATE INDEX "officer_society_id_idx" ON "officer" ("society_id");
CREATE INDEX "plot_rent_due_on_id_idx" ON "plot" ("rent_due_on", "id");
CREATE INDEX "plot_society_id_number_id_idx" ON "plot" ("society_id", "number", "id");
CREATE INDEX "reminder_plot_id_idx" ON "reminder" ("plot_id");
CREATE INDEX "reminder_society_id_idx" ON "reminder" ("society_id");
CREATE INDEX "rent_round_society_id_idx" ON "rent_round" ("society_id");
