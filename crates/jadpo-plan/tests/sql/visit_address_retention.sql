-- the retention of `Visit.address`: after setting 1, store main
UPDATE "visit"
SET "address" = NULL,
  "changed_at" = $2
WHERE "id" IN (
  SELECT "id"
  FROM "visit"
  WHERE "address" IS NOT NULL
    AND "at" < $1
  ORDER BY "at" ASC, "id" ASC
  LIMIT 500
  FOR UPDATE SKIP LOCKED
)
RETURNING "id"
-- $1: Cutoff as Timestamp
-- $2: Now as Timestamp
