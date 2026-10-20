-- 20261020T1000_estimate_runs_and_walk_plots_by_id: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
DROP INDEX "plot_society_id_number_id_idx";
ALTER TABLE "jadpo_batch_runs" ADD COLUMN "estimated_total" bigint;
CREATE INDEX "plot_society_id_id_idx" ON "plot" ("society_id", "id");
