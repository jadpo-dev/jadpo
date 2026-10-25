-- 20261025T0215_initial: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
CREATE TABLE "baker" (
    "id" uuid NOT NULL,
    "handle" text NOT NULL,
    CONSTRAINT "baker_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "bakery" (
    "id" uuid NOT NULL,
    "name" text NOT NULL,
    CONSTRAINT "bakery_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "ingredient" (
    "id" uuid NOT NULL,
    "name" text NOT NULL,
    "allergen" text,
    CONSTRAINT "ingredient_pkey" PRIMARY KEY ("id")
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
CREATE TABLE "jadpo_sessions" (
    "id" uuid NOT NULL,
    "principal_id" uuid NOT NULL,
    "verifier" text NOT NULL,
    "csrf_verifier" text,
    "started_at" timestamptz(3) NOT NULL,
    "ends_at" timestamptz(3) NOT NULL,
    CONSTRAINT "jadpo_sessions_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "platform_staff" (
    "id" uuid NOT NULL,
    "baker_id" uuid NOT NULL,
    "role" text NOT NULL,
    CONSTRAINT "platform_staff_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "recipe" (
    "id" uuid NOT NULL,
    "bakery_id" uuid NOT NULL,
    "author_id" uuid NOT NULL,
    "name" text NOT NULL,
    "batch_cost_amount" bigint NOT NULL,
    "batch_cost_currency" char(3) NOT NULL,
    "old_reference" text,
    CONSTRAINT "recipe_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "staff_member" (
    "id" uuid NOT NULL,
    "bakery_id" uuid NOT NULL,
    "baker_id" uuid NOT NULL,
    "role" text NOT NULL,
    "joined_at" timestamptz(3) NOT NULL,
    CONSTRAINT "staff_member_pkey" PRIMARY KEY ("id")
);
ALTER TABLE "baker" ADD CONSTRAINT "baker_handle_key" UNIQUE ("handle");
ALTER TABLE "ingredient" ADD CONSTRAINT "ingredient_name_key" UNIQUE ("name");
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_subscriber_key" UNIQUE ("outbox_id", "subscriber");
ALTER TABLE "jadpo_sessions" ADD CONSTRAINT "jadpo_sessions_verifier_key" UNIQUE ("verifier");
ALTER TABLE "recipe" ADD CONSTRAINT "recipe_batch_cost_currency_check" CHECK ("batch_cost_currency" = 'GBP');
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_fkey" FOREIGN KEY ("outbox_id") REFERENCES "jadpo_outbox" ("id") ON DELETE CASCADE;
ALTER TABLE "platform_staff" ADD CONSTRAINT "platform_staff_baker_id_fkey" FOREIGN KEY ("baker_id") REFERENCES "baker" ("id") ON DELETE CASCADE;
ALTER TABLE "recipe" ADD CONSTRAINT "recipe_author_id_fkey" FOREIGN KEY ("author_id") REFERENCES "baker" ("id") ON DELETE RESTRICT;
ALTER TABLE "recipe" ADD CONSTRAINT "recipe_bakery_id_fkey" FOREIGN KEY ("bakery_id") REFERENCES "bakery" ("id") ON DELETE CASCADE;
ALTER TABLE "staff_member" ADD CONSTRAINT "staff_member_baker_id_fkey" FOREIGN KEY ("baker_id") REFERENCES "baker" ("id") ON DELETE CASCADE;
ALTER TABLE "staff_member" ADD CONSTRAINT "staff_member_bakery_id_fkey" FOREIGN KEY ("bakery_id") REFERENCES "bakery" ("id") ON DELETE CASCADE;
CREATE INDEX "bakery_name_id_sorted_idx" ON "bakery" ("name" COLLATE "C", "id");
CREATE INDEX "ingredient_name_sorted_idx" ON "ingredient" ("name" COLLATE "C");
CREATE INDEX "jadpo_deliveries_status_next_attempt_at_idx" ON "jadpo_deliveries" ("status", "next_attempt_at");
CREATE INDEX "jadpo_idempotency_keys_kept_until_idx" ON "jadpo_idempotency_keys" ("kept_until");
CREATE INDEX "jadpo_operator_actions_item_idx" ON "jadpo_operator_actions" ("item");
CREATE INDEX "jadpo_outbox_emitted_at_idx" ON "jadpo_outbox" ("emitted_at");
CREATE INDEX "jadpo_sessions_ends_at_idx" ON "jadpo_sessions" ("ends_at");
CREATE INDEX "jadpo_sessions_principal_id_idx" ON "jadpo_sessions" ("principal_id");
CREATE INDEX "platform_staff_baker_id_idx" ON "platform_staff" ("baker_id");
CREATE INDEX "recipe_author_id_idx" ON "recipe" ("author_id");
CREATE INDEX "recipe_bakery_id_name_id_sorted_idx" ON "recipe" ("bakery_id", "name" COLLATE "C", "id");
CREATE INDEX "staff_member_baker_id_idx" ON "staff_member" ("baker_id");
CREATE INDEX "staff_member_bakery_id_joined_at_id_idx" ON "staff_member" ("bakery_id", "joined_at", "id");
