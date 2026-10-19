-- 20261019T2317_keep_batch_run_encoding: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
ALTER TABLE "jadpo_batch_runs" ADD COLUMN "encoding" smallint;
