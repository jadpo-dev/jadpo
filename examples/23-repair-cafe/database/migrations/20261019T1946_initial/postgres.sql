-- 20261019T1946_initial: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
CREATE TABLE "bench" (
    "id" uuid NOT NULL,
    "item" text NOT NULL,
    "fixer" text,
    CONSTRAINT "bench_pkey" PRIMARY KEY ("id")
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
CREATE TABLE "repair" (
    "id" uuid NOT NULL,
    "item" text NOT NULL,
    "fault" text NOT NULL,
    "bench_id" uuid,
    "ticket_printed_at" timestamptz(3),
    "booked_at" timestamptz(3) NOT NULL,
    CONSTRAINT "repair_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "thank_you_note" (
    "id" uuid NOT NULL,
    "repair_id" uuid NOT NULL,
    "note" text NOT NULL,
    CONSTRAINT "thank_you_note_pkey" PRIMARY KEY ("id")
);
ALTER TABLE "bench" ADD CONSTRAINT "bench_item_key" UNIQUE ("item");
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_subscriber_key" UNIQUE ("outbox_id", "subscriber");
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_fkey" FOREIGN KEY ("outbox_id") REFERENCES "jadpo_outbox" ("id") ON DELETE CASCADE;
ALTER TABLE "repair" ADD CONSTRAINT "repair_bench_id_fkey" FOREIGN KEY ("bench_id") REFERENCES "bench" ("id") ON DELETE SET NULL;
ALTER TABLE "thank_you_note" ADD CONSTRAINT "thank_you_note_repair_id_fkey" FOREIGN KEY ("repair_id") REFERENCES "repair" ("id") ON DELETE CASCADE;
CREATE INDEX "jadpo_deliveries_status_next_attempt_at_idx" ON "jadpo_deliveries" ("status", "next_attempt_at");
CREATE INDEX "jadpo_idempotency_keys_kept_until_idx" ON "jadpo_idempotency_keys" ("kept_until");
CREATE INDEX "jadpo_outbox_emitted_at_idx" ON "jadpo_outbox" ("emitted_at");
CREATE INDEX "repair_bench_id_idx" ON "repair" ("bench_id");
CREATE INDEX "thank_you_note_repair_id_idx" ON "thank_you_note" ("repair_id");
