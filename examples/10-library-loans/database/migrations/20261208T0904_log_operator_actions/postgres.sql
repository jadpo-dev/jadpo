-- 20261208T0904_log_operator_actions: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
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
ALTER TABLE "jadpo_deliveries" ADD COLUMN "retried_at" timestamptz(3);
CREATE INDEX "jadpo_operator_actions_item_idx" ON "jadpo_operator_actions" ("item");
