-- 20261124T0900_record_what_fields_allow: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
DROP INDEX "book_title_id_idx";
CREATE INDEX "book_title_id_sorted_idx" ON "book" ("title" COLLATE "C", "id");
