-- the retention of `Visit.spent`: after setting 1, store main
UPDATE "visit"
SET "spent_amount" = NULL,
  "spent_currency" = NULL,
  "changed_at" = $2
WHERE "id" IN (
  SELECT "id"
  FROM "visit"
  WHERE ("spent_amount" IS NOT NULL OR "spent_currency" IS NOT NULL)
    AND "at" < $1
  ORDER BY "at" ASC, "id" ASC
  LIMIT 500
  FOR UPDATE SKIP LOCKED
)
RETURNING "id"
-- $1: Cutoff as Timestamp
-- $2: Now as Timestamp
