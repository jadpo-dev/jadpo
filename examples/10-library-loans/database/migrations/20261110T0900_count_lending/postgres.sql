-- 20261110T0900_count_lending: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
ALTER TABLE "book" ADD COLUMN "last_lent_at" timestamptz(3);
ALTER TABLE "book" ADD COLUMN "times_lent" bigint;
UPDATE "book" SET "times_lent" = 0 WHERE "times_lent" IS NULL;
ALTER TABLE "book" ALTER COLUMN "times_lent" SET NOT NULL;
