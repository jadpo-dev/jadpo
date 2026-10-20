-- 20261020T0901_index_plot_reminders: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
DROP INDEX "reminder_plot_id_idx";
CREATE INDEX "reminder_plot_id_sent_on_id_idx" ON "reminder" ("plot_id", "sent_on", "id");
