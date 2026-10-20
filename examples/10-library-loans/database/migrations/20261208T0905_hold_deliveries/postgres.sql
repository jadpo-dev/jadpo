-- 20261208T0905_hold_deliveries: written by `jadpo migrate new` from the snapshots; never edit it by hand.

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
ALTER TABLE "jadpo_deliveries" ADD COLUMN "held_for" text;
ALTER TABLE "jadpo_deliveries" ADD COLUMN "held_ms" bigint;
ALTER TABLE "jadpo_deliveries" ADD COLUMN "held_since" timestamptz(3);
