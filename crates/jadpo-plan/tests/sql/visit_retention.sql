-- the retention of `Visit`: after setting 0, store main
DELETE FROM "visit"
WHERE "id" IN (
  SELECT "id"
  FROM "visit"
  WHERE "at" < $1
  ORDER BY "at" ASC, "id" ASC
  LIMIT 500
  FOR UPDATE SKIP LOCKED
)
RETURNING "id"
-- $1: Cutoff as Timestamp
