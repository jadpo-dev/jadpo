-- 20261020T2247_initial: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
CREATE EXTENSION IF NOT EXISTS "vector";
CREATE TABLE "found_item" (
    "id" uuid NOT NULL,
    "what_was_found" text NOT NULL,
    "where_found" text NOT NULL,
    "handed_in_on" date NOT NULL,
    "returned_to_owner" boolean NOT NULL,
    "description_meaning" vector(256),
    CONSTRAINT "found_item_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "jadpo_ai_usage" (
    "day" date NOT NULL,
    "scope" text NOT NULL,
    "key" text NOT NULL,
    "calls" integer NOT NULL,
    "input_tokens" bigint NOT NULL,
    "output_tokens" bigint NOT NULL,
    "cached_tokens" bigint NOT NULL,
    "reserved_tokens" bigint NOT NULL,
    "cost_micros" bigint NOT NULL,
    "reserved_micros" bigint NOT NULL,
    "currency" text,
    "touched_at" timestamptz(3) NOT NULL,
    CONSTRAINT "jadpo_ai_usage_pkey" PRIMARY KEY ("day", "scope", "key")
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
CREATE TABLE "loss_report" (
    "id" uuid NOT NULL,
    "passenger_name" text NOT NULL,
    "passenger_email" text NOT NULL,
    "what_was_lost" text NOT NULL,
    "lost_on" date NOT NULL,
    "reader_locale" text NOT NULL,
    "likely_found" boolean,
    "drafted_reply" text,
    CONSTRAINT "loss_report_pkey" PRIMARY KEY ("id")
);
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_subscriber_key" UNIQUE ("outbox_id", "subscriber");
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_fkey" FOREIGN KEY ("outbox_id") REFERENCES "jadpo_outbox" ("id") ON DELETE CASCADE;
CREATE INDEX "found_item_handed_in_on_id_idx" ON "found_item" ("handed_in_on", "id");
CREATE INDEX "found_item_returned_to_owner_handed_in_on_id_idx" ON "found_item" ("returned_to_owner", "handed_in_on", "id");
CREATE INDEX "jadpo_deliveries_status_next_attempt_at_idx" ON "jadpo_deliveries" ("status", "next_attempt_at");
CREATE INDEX "jadpo_idempotency_keys_kept_until_idx" ON "jadpo_idempotency_keys" ("kept_until");
CREATE INDEX "jadpo_operator_actions_item_idx" ON "jadpo_operator_actions" ("item");
CREATE INDEX "jadpo_outbox_emitted_at_idx" ON "jadpo_outbox" ("emitted_at");
