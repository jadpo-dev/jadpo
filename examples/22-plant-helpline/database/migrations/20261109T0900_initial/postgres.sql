-- 20261109T0900_initial: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
CREATE EXTENSION IF NOT EXISTS "vector";
CREATE TABLE "care_guide" (
    "id" uuid NOT NULL,
    "slug" text NOT NULL,
    "title" text NOT NULL,
    "care_advice" text NOT NULL,
    "on_help_site" boolean NOT NULL,
    "advice_meaning" vector(4),
    CONSTRAINT "care_guide_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "jadpo_deliveries" (
    "id" uuid NOT NULL,
    "outbox_id" uuid NOT NULL,
    "subscriber" text NOT NULL,
    "status" text NOT NULL,
    "attempts" integer NOT NULL,
    "next_attempt_at" timestamptz(3),
    "last_failure" jsonb,
    "last_tried_at" timestamptz(3) NOT NULL,
    CONSTRAINT "jadpo_deliveries_pkey" PRIMARY KEY ("id")
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
    CONSTRAINT "jadpo_outbox_pkey" PRIMARY KEY ("id")
);
ALTER TABLE "care_guide" ADD CONSTRAINT "care_guide_slug_key" UNIQUE ("slug");
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_subscriber_key" UNIQUE ("outbox_id", "subscriber");
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_fkey" FOREIGN KEY ("outbox_id") REFERENCES "jadpo_outbox" ("id") ON DELETE CASCADE;
CREATE INDEX "care_guide_by_meaning_idx" ON "care_guide" USING hnsw ("advice_meaning" vector_cosine_ops) WITH (m = 24, ef_construction = 128);
CREATE INDEX "care_guide_on_help_site_idx" ON "care_guide" ("on_help_site");
CREATE INDEX "jadpo_deliveries_status_next_attempt_at_idx" ON "jadpo_deliveries" ("status", "next_attempt_at");
CREATE INDEX "jadpo_idempotency_keys_kept_until_idx" ON "jadpo_idempotency_keys" ("kept_until");
CREATE INDEX "jadpo_outbox_emitted_at_idx" ON "jadpo_outbox" ("emitted_at");
