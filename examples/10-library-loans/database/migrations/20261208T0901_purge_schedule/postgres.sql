-- 20261208T0901_purge_schedule: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
CREATE TABLE "jadpo_schedule_runs" (
    "id" uuid NOT NULL,
    "schedule" text NOT NULL,
    "due_slot" timestamptz(3) NOT NULL,
    "started_at" timestamptz(3) NOT NULL,
    "ended_at" timestamptz(3),
    "outcome" text NOT NULL,
    "reason" text,
    "failure" jsonb,
    "events" integer,
    "traceparent" text,
    "items" integer,
    CONSTRAINT "jadpo_schedule_runs_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "jadpo_schedules" (
    "schedule" text NOT NULL,
    "every_ms" bigint NOT NULL,
    "next_run_at" timestamptz(3) NOT NULL,
    "paused" boolean NOT NULL,
    "lease_until" timestamptz(3),
    "run_id" uuid,
    "last_tick_at" timestamptz(3),
    "last_succeeded_at" timestamptz(3),
    CONSTRAINT "jadpo_schedules_pkey" PRIMARY KEY ("schedule")
);
CREATE INDEX "jadpo_schedule_runs_schedule_started_at_idx" ON "jadpo_schedule_runs" ("schedule", "started_at");
CREATE INDEX "jadpo_schedule_runs_started_at_idx" ON "jadpo_schedule_runs" ("started_at");
