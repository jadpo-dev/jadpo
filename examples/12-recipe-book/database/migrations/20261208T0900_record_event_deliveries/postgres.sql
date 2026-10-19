-- 20261208T0900_record_event_deliveries: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
CREATE TABLE "jadpo_handled_deliveries" (
    "id" uuid NOT NULL,
    "handled_at" timestamptz(3) NOT NULL,
    CONSTRAINT "jadpo_handled_deliveries_pkey" PRIMARY KEY ("id")
);
ALTER TABLE "jadpo_deliveries" ALTER COLUMN "last_tried_at" DROP NOT NULL;
ALTER TABLE "jadpo_outbox" ADD COLUMN "depth" integer;
