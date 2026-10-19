-- 20261102T0900_initial: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
CREATE EXTENSION IF NOT EXISTS "postgis";
CREATE TABLE "delivery" (
    "id" uuid NOT NULL,
    "drop_point_id" uuid NOT NULL,
    "delivered_on" date NOT NULL,
    "boxes" smallint NOT NULL,
    "fee_amount" bigint NOT NULL,
    "fee_currency" char(3) NOT NULL,
    CONSTRAINT "delivery_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "drop_point" (
    "id" uuid NOT NULL,
    "name" text NOT NULL,
    "spot" geography NOT NULL,
    CONSTRAINT "drop_point_pkey" PRIMARY KEY ("id")
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
CREATE TABLE "van_round" (
    "id" uuid NOT NULL,
    "name" text NOT NULL,
    "outline" geography NOT NULL,
    CONSTRAINT "van_round_pkey" PRIMARY KEY ("id")
);
ALTER TABLE "delivery" ADD CONSTRAINT "delivery_fee_currency_check" CHECK ("fee_currency" = 'GBP');
ALTER TABLE "drop_point" ADD CONSTRAINT "drop_point_spot_check" CHECK (ST_SRID("spot") = 4326 AND GeometryType("spot") = 'POINT');
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_subscriber_key" UNIQUE ("outbox_id", "subscriber");
ALTER TABLE "van_round" ADD CONSTRAINT "van_round_outline_check" CHECK (ST_SRID("outline") = 4326 AND GeometryType("outline") = 'POLYGON');
ALTER TABLE "delivery" ADD CONSTRAINT "delivery_drop_point_id_fkey" FOREIGN KEY ("drop_point_id") REFERENCES "drop_point" ("id") ON DELETE RESTRICT;
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_fkey" FOREIGN KEY ("outbox_id") REFERENCES "jadpo_outbox" ("id") ON DELETE CASCADE;
CREATE INDEX "delivery_drop_point_id_idx" ON "delivery" ("drop_point_id");
CREATE INDEX "drop_point_by_spot_idx" ON "drop_point" USING gist ("spot");
CREATE INDEX "drop_point_name_id_sorted_idx" ON "drop_point" ("name" COLLATE "C", "id");
CREATE INDEX "drop_point_spot_box_idx" ON "drop_point" USING gist (("spot"::geometry));
CREATE INDEX "jadpo_deliveries_status_next_attempt_at_idx" ON "jadpo_deliveries" ("status", "next_attempt_at");
CREATE INDEX "jadpo_idempotency_keys_kept_until_idx" ON "jadpo_idempotency_keys" ("kept_until");
CREATE INDEX "jadpo_outbox_emitted_at_idx" ON "jadpo_outbox" ("emitted_at");
CREATE INDEX "van_round_name_id_sorted_idx" ON "van_round" ("name" COLLATE "C", "id");
CREATE INDEX "van_round_outline_idx" ON "van_round" USING gist ("outline");
