-- 20261023T0816_reminder_dispatch_metadata: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
CREATE TABLE "jadpo_step_records" (
    "id" uuid NOT NULL,
    "scope" text NOT NULL,
    "key" text NOT NULL,
    "action" text NOT NULL,
    "build" text NOT NULL,
    "shape" text NOT NULL,
    "started_at" timestamptz(3) NOT NULL,
    "generation" bigint NOT NULL,
    "state" text NOT NULL,
    "problem" jsonb,
    "taken_at" timestamptz(3) NOT NULL,
    "alerted_at" timestamptz(3),
    CONSTRAINT "jadpo_step_records_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "jadpo_steps" (
    "record_id" uuid NOT NULL,
    "position" integer NOT NULL,
    "operation" text NOT NULL,
    "fingerprint" jsonb NOT NULL,
    "key_generation" integer NOT NULL,
    "dispatched_at" timestamptz(3) NOT NULL,
    "outcome" text,
    "answer" bytea,
    "answered_at" timestamptz(3),
    CONSTRAINT "jadpo_steps_pkey" PRIMARY KEY ("record_id", "position")
);
ALTER TABLE "jadpo_step_records" ADD CONSTRAINT "jadpo_step_records_scope_key_key" UNIQUE ("scope", "key");
ALTER TABLE "jadpo_steps" ADD CONSTRAINT "jadpo_steps_record_id_fkey" FOREIGN KEY ("record_id") REFERENCES "jadpo_step_records" ("id") ON DELETE CASCADE;
