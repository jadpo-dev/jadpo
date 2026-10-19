-- 20261019T2124_request_schedule_runs: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
ALTER TABLE "jadpo_schedules" ADD COLUMN "run_requested" uuid;
