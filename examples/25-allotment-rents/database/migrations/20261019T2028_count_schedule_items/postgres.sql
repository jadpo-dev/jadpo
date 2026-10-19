-- 20261019T2028_count_schedule_items: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
ALTER TABLE "jadpo_schedule_runs" ADD COLUMN "items" integer;
