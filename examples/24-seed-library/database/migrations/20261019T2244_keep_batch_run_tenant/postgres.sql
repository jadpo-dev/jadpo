-- 20261019T2244_keep_batch_run_tenant: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
ALTER TABLE "jadpo_batch_runs" DROP CONSTRAINT "jadpo_batch_runs_batch_key_key";
ALTER TABLE "jadpo_batch_runs" ADD COLUMN "tenant" uuid;
ALTER TABLE "jadpo_batch_runs" ADD CONSTRAINT "jadpo_batch_runs_batch_tenant_key_key" UNIQUE ("batch", "tenant", "key");
