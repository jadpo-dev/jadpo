-- 20261020T0057_index_pinned_notes: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
DROP INDEX "thank_you_note_repair_id_idx";
CREATE INDEX "thank_you_note_repair_id_id_idx" ON "thank_you_note" ("repair_id", "id");
