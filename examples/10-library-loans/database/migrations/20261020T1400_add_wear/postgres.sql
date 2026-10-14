-- 20261020T1400_add_wear: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
ALTER TABLE "book" ALTER COLUMN "shelf_code" DROP NOT NULL;
ALTER TABLE "book" ADD COLUMN "wear" text;
UPDATE "book" SET "wear" = 'pristine' WHERE "wear" IS NULL;
ALTER TABLE "book" ALTER COLUMN "wear" SET NOT NULL;
