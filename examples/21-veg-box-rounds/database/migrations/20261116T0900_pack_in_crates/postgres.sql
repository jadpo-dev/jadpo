-- 20261116T0900_pack_in_crates: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
ALTER TABLE "delivery" ADD COLUMN "crates_used" bigint;
-- backfill Delivery.crates_used: by UPDATE, 1000 rows a batch, each batch its own transaction
SELECT count(*) FROM "delivery" WHERE "crates_used" IS NULL;
WITH "batch" AS (SELECT "id" AS "jadpo_key" FROM "delivery" WHERE "id" > $1 ORDER BY "id" LIMIT 1000), "filled" AS (UPDATE "delivery" SET "crates_used" = (((("delivery"."boxes")::bigint) / ((6)::bigint)) + CASE WHEN ((("delivery"."boxes")::bigint) % ((6)::bigint)) = 0 THEN 0 WHEN TRUE THEN (CASE WHEN ((("delivery"."boxes")::bigint) < 0) = (((6)::bigint) < 0) THEN 1 ELSE -1 END) ELSE 0 END) FROM "batch" WHERE "delivery"."id" = "batch"."jadpo_key" AND "delivery"."crates_used" IS NULL RETURNING 1) SELECT (SELECT "jadpo_key" FROM "batch" ORDER BY "jadpo_key" DESC LIMIT 1), (SELECT count(*) FROM "filled");
LOCK TABLE "delivery" IN SHARE ROW EXCLUSIVE MODE;
UPDATE "delivery" SET "crates_used" = (((("delivery"."boxes")::bigint) / ((6)::bigint)) + CASE WHEN ((("delivery"."boxes")::bigint) % ((6)::bigint)) = 0 THEN 0 WHEN TRUE THEN (CASE WHEN ((("delivery"."boxes")::bigint) < 0) = (((6)::bigint) < 0) THEN 1 ELSE -1 END) ELSE 0 END) WHERE "crates_used" IS NULL;
ALTER TABLE "delivery" ALTER COLUMN "crates_used" SET NOT NULL;
