-- 20261020T1000_keep_batch_run_estimate: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
ALTER TABLE "jadpo_batch_runs" ADD COLUMN "estimated_total" bigint;
