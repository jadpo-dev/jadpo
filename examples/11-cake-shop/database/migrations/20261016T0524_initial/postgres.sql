-- 20261016T0524_initial: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
CREATE TABLE "cake" (
    "id" uuid NOT NULL,
    "name" text NOT NULL,
    "flavour" text NOT NULL,
    "price" bigint NOT NULL,
    "on_menu" boolean NOT NULL,
    CONSTRAINT "cake_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "customer" (
    "id" uuid NOT NULL,
    "email" text NOT NULL,
    "name" text NOT NULL,
    "status" text NOT NULL,
    CONSTRAINT "customer_pkey" PRIMARY KEY ("id")
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
CREATE TABLE "order" (
    "id" uuid NOT NULL,
    "customer_id" uuid NOT NULL,
    "outlet_id" uuid NOT NULL,
    "cake_id" uuid NOT NULL,
    "quantity" smallint NOT NULL,
    "collect_on" date NOT NULL,
    "placed_with" text,
    "ordered_at" timestamptz(3) NOT NULL,
    CONSTRAINT "order_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "outlet" (
    "id" uuid NOT NULL,
    "name" text NOT NULL,
    "status" text NOT NULL,
    CONSTRAINT "outlet_pkey" PRIMARY KEY ("id")
);
ALTER TABLE "customer" ADD CONSTRAINT "customer_email_key" UNIQUE ("email");
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_subscriber_key" UNIQUE ("outbox_id", "subscriber");
ALTER TABLE "jadpo_service_keys" ADD CONSTRAINT "jadpo_service_keys_verifier_key" UNIQUE ("verifier");
ALTER TABLE "jadpo_sessions" ADD CONSTRAINT "jadpo_sessions_verifier_key" UNIQUE ("verifier");
ALTER TABLE "outlet" ADD CONSTRAINT "outlet_name_key" UNIQUE ("name");
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_fkey" FOREIGN KEY ("outbox_id") REFERENCES "jadpo_outbox" ("id") ON DELETE CASCADE;
ALTER TABLE "order" ADD CONSTRAINT "order_cake_id_fkey" FOREIGN KEY ("cake_id") REFERENCES "cake" ("id") ON DELETE RESTRICT;
ALTER TABLE "order" ADD CONSTRAINT "order_customer_id_fkey" FOREIGN KEY ("customer_id") REFERENCES "customer" ("id") ON DELETE CASCADE;
ALTER TABLE "order" ADD CONSTRAINT "order_outlet_id_fkey" FOREIGN KEY ("outlet_id") REFERENCES "outlet" ("id") ON DELETE RESTRICT;
CREATE INDEX "cake_on_menu_name_id_sorted_idx" ON "cake" ("on_menu", "name" COLLATE "C", "id");
CREATE INDEX "jadpo_deliveries_status_next_attempt_at_idx" ON "jadpo_deliveries" ("status", "next_attempt_at");
CREATE INDEX "jadpo_idempotency_keys_kept_until_idx" ON "jadpo_idempotency_keys" ("kept_until");
CREATE INDEX "jadpo_outbox_emitted_at_idx" ON "jadpo_outbox" ("emitted_at");
CREATE INDEX "jadpo_service_keys_principal_id_idx" ON "jadpo_service_keys" ("principal_id");
CREATE INDEX "jadpo_sessions_ends_at_idx" ON "jadpo_sessions" ("ends_at");
CREATE INDEX "jadpo_sessions_principal_id_idx" ON "jadpo_sessions" ("principal_id");
CREATE INDEX "order_cake_id_idx" ON "order" ("cake_id");
CREATE INDEX "order_collect_on_id_idx" ON "order" ("collect_on", "id");
CREATE INDEX "order_collect_on_ordered_at_id_idx" ON "order" ("collect_on", "ordered_at", "id");
CREATE INDEX "order_customer_id_idx" ON "order" ("customer_id");
CREATE INDEX "order_outlet_id_idx" ON "order" ("outlet_id");
CREATE INDEX "outlet_status_name_sorted_idx" ON "outlet" ("status", "name" COLLATE "C");
