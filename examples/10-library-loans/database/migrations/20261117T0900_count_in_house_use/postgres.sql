-- 20261117T0900_count_in_house_use: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
ALTER TABLE "book" ADD COLUMN "in_house_uses" bigint;
UPDATE "book" SET "in_house_uses" = 0 WHERE "in_house_uses" IS NULL;
ALTER TABLE "book" ALTER COLUMN "in_house_uses" SET NOT NULL;
