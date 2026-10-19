-- 20261019T2016_initial: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
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
CREATE TABLE "seed_packet" (
    "id" uuid NOT NULL,
    "variety" text NOT NULL,
    "saved_in" smallint NOT NULL,
    "donated_on" date NOT NULL,
    "on_loan" boolean NOT NULL,
    "withdrawn" boolean NOT NULL,
    CONSTRAINT "seed_packet_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "stock_check" (
    "id" uuid NOT NULL,
    "season" smallint NOT NULL,
    "shelf_walk" jsonb,
    "packets_checked" bigint,
    "packets_on_loan" bigint,
    CONSTRAINT "stock_check_pkey" PRIMARY KEY ("id")
);
ALTER TABLE "jadpo_batch_runs" ADD CONSTRAINT "jadpo_batch_runs_batch_key_key" UNIQUE ("batch", "key");
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_subscriber_key" UNIQUE ("outbox_id", "subscriber");
ALTER TABLE "jadpo_batch_problems" ADD CONSTRAINT "jadpo_batch_problems_run_id_fkey" FOREIGN KEY ("run_id") REFERENCES "jadpo_batch_runs" ("id") ON DELETE CASCADE;
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_fkey" FOREIGN KEY ("outbox_id") REFERENCES "jadpo_outbox" ("id") ON DELETE CASCADE;
CREATE INDEX "jadpo_batch_runs_state_lease_until_idx" ON "jadpo_batch_runs" ("state", "lease_until");
CREATE INDEX "jadpo_deliveries_status_next_attempt_at_idx" ON "jadpo_deliveries" ("status", "next_attempt_at");
CREATE INDEX "jadpo_idempotency_keys_kept_until_idx" ON "jadpo_idempotency_keys" ("kept_until");
CREATE INDEX "jadpo_outbox_emitted_at_idx" ON "jadpo_outbox" ("emitted_at");
CREATE INDEX "seed_packet_donated_on_id_idx" ON "seed_packet" ("donated_on", "id");
CREATE INDEX "seed_packet_donated_on_year_idx" ON "seed_packet" (((CASE WHEN extract(year FROM "donated_on") < 0 THEN extract(year FROM "donated_on") + 1 ELSE extract(year FROM "donated_on") END)::bigint));
CREATE INDEX "seed_packet_withdrawn_id_idx" ON "seed_packet" ("withdrawn", "id");
