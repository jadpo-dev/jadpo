-- 20261112T0951_keep_lease_holders: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
ALTER TABLE "jadpo_schedules" ADD COLUMN "lease_holder" text;
