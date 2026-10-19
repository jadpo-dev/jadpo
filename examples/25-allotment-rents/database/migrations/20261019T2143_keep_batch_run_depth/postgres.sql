-- 20261019T2143_keep_batch_run_depth: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
ALTER TABLE "jadpo_batch_runs" ADD COLUMN "depth" integer;
