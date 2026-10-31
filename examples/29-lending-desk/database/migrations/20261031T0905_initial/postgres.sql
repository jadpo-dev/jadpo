-- 20261031T0905_initial: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
CREATE TABLE "book" (
    "id" uuid NOT NULL,
    "title" text NOT NULL,
    "author" text NOT NULL,
    "shelf" text NOT NULL,
    "donor" text,
    CONSTRAINT "book_pkey" PRIMARY KEY ("id")
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
    "depth" integer,
    "tenant" uuid,
    "encoding" smallint,
    "estimated_total" bigint,
    CONSTRAINT "jadpo_batch_runs_pkey" PRIMARY KEY ("id")
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
CREATE TABLE "jadpo_provider_sign_ins" (
    "id" uuid NOT NULL,
    "provider" text NOT NULL,
    "state" text NOT NULL,
    "binding" text NOT NULL,
    "return_to" text NOT NULL,
    "expires_at" timestamptz(3) NOT NULL,
    CONSTRAINT "jadpo_provider_sign_ins_pkey" PRIMARY KEY ("id")
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
CREATE TABLE "jadpo_sessions" (
    "id" uuid NOT NULL,
    "principal_id" uuid NOT NULL,
    "verifier" text NOT NULL,
    "csrf_verifier" text,
    "started_at" timestamptz(3) NOT NULL,
    "ends_at" timestamptz(3) NOT NULL,
    CONSTRAINT "jadpo_sessions_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "jadpo_sign_in_links" (
    "method" text NOT NULL,
    "subject" text NOT NULL,
    "user_id" uuid NOT NULL,
    "linked_at" timestamptz(3) NOT NULL,
    CONSTRAINT "jadpo_sign_in_links_pkey" PRIMARY KEY ("method", "subject")
);
CREATE TABLE "jadpo_sign_in_requests" (
    "id" uuid NOT NULL,
    "address" text NOT NULL,
    "exact" text,
    "request" uuid,
    "token" text,
    "code" text,
    "sealed" bytea,
    "return_to" text,
    "binding" text,
    "wrong_codes" integer NOT NULL,
    "requested_at" timestamptz(3),
    "expires_at" timestamptz(3),
    "sends_since" timestamptz(3),
    "sends" integer NOT NULL,
    "failed_since" timestamptz(3),
    "failed" integer NOT NULL,
    "kept_until" timestamptz(3) NOT NULL,
    CONSTRAINT "jadpo_sign_in_requests_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "loan" (
    "id" uuid NOT NULL,
    "book_id" uuid NOT NULL,
    "borrower_id" uuid NOT NULL,
    "due_at" timestamptz(3) NOT NULL,
    "reminded_at" timestamptz(3),
    CONSTRAINT "loan_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "patron" (
    "id" uuid NOT NULL,
    "email" text NOT NULL,
    "name" text,
    "active" boolean NOT NULL,
    CONSTRAINT "patron_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "staff_member" (
    "id" uuid NOT NULL,
    "patron_id" uuid NOT NULL,
    "role" text NOT NULL,
    CONSTRAINT "staff_member_pkey" PRIMARY KEY ("id")
);
ALTER TABLE "jadpo_batch_runs" ADD CONSTRAINT "jadpo_batch_runs_batch_tenant_key_key" UNIQUE ("batch", "tenant", "key");
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_subscriber_key" UNIQUE ("outbox_id", "subscriber");
ALTER TABLE "jadpo_provider_sign_ins" ADD CONSTRAINT "jadpo_provider_sign_ins_state_key" UNIQUE ("state");
ALTER TABLE "jadpo_sessions" ADD CONSTRAINT "jadpo_sessions_verifier_key" UNIQUE ("verifier");
ALTER TABLE "jadpo_sign_in_requests" ADD CONSTRAINT "jadpo_sign_in_requests_address_key" UNIQUE ("address");
ALTER TABLE "jadpo_sign_in_requests" ADD CONSTRAINT "jadpo_sign_in_requests_request_key" UNIQUE ("request");
ALTER TABLE "jadpo_sign_in_requests" ADD CONSTRAINT "jadpo_sign_in_requests_token_key" UNIQUE ("token");
ALTER TABLE "loan" ADD CONSTRAINT "loan_book_id_key" UNIQUE ("book_id");
ALTER TABLE "patron" ADD CONSTRAINT "patron_email_key" UNIQUE ("email");
ALTER TABLE "jadpo_batch_problems" ADD CONSTRAINT "jadpo_batch_problems_run_id_fkey" FOREIGN KEY ("run_id") REFERENCES "jadpo_batch_runs" ("id") ON DELETE CASCADE;
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_fkey" FOREIGN KEY ("outbox_id") REFERENCES "jadpo_outbox" ("id") ON DELETE CASCADE;
ALTER TABLE "loan" ADD CONSTRAINT "loan_book_id_fkey" FOREIGN KEY ("book_id") REFERENCES "book" ("id") ON DELETE RESTRICT;
ALTER TABLE "loan" ADD CONSTRAINT "loan_borrower_id_fkey" FOREIGN KEY ("borrower_id") REFERENCES "patron" ("id") ON DELETE RESTRICT;
ALTER TABLE "staff_member" ADD CONSTRAINT "staff_member_patron_id_fkey" FOREIGN KEY ("patron_id") REFERENCES "patron" ("id") ON DELETE CASCADE;
CREATE INDEX "book_shelf_title_id_sorted_idx" ON "book" ("shelf" COLLATE "C", "title" COLLATE "C", "id");
CREATE INDEX "book_title_id_sorted_idx" ON "book" ("title" COLLATE "C", "id");
CREATE INDEX "jadpo_batch_runs_state_lease_until_idx" ON "jadpo_batch_runs" ("state", "lease_until");
CREATE INDEX "jadpo_deliveries_status_next_attempt_at_idx" ON "jadpo_deliveries" ("status", "next_attempt_at");
CREATE INDEX "jadpo_idempotency_keys_kept_until_idx" ON "jadpo_idempotency_keys" ("kept_until");
CREATE INDEX "jadpo_operator_actions_item_idx" ON "jadpo_operator_actions" ("item");
CREATE INDEX "jadpo_outbox_emitted_at_idx" ON "jadpo_outbox" ("emitted_at");
CREATE INDEX "jadpo_provider_sign_ins_expires_at_idx" ON "jadpo_provider_sign_ins" ("expires_at");
CREATE INDEX "jadpo_schedule_runs_schedule_started_at_idx" ON "jadpo_schedule_runs" ("schedule", "started_at");
CREATE INDEX "jadpo_schedule_runs_started_at_idx" ON "jadpo_schedule_runs" ("started_at");
CREATE INDEX "jadpo_sessions_ends_at_idx" ON "jadpo_sessions" ("ends_at");
CREATE INDEX "jadpo_sessions_principal_id_idx" ON "jadpo_sessions" ("principal_id");
CREATE INDEX "jadpo_sign_in_links_user_id_idx" ON "jadpo_sign_in_links" ("user_id");
CREATE INDEX "jadpo_sign_in_requests_kept_until_idx" ON "jadpo_sign_in_requests" ("kept_until");
CREATE INDEX "loan_borrower_id_due_at_id_idx" ON "loan" ("borrower_id", "due_at", "id");
CREATE INDEX "staff_member_patron_id_id_idx" ON "staff_member" ("patron_id", "id");
