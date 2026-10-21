-- 20261020T2348_keep_poll_interval: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
ALTER TABLE "jadpo_polls" ADD COLUMN "every_ms" bigint;
