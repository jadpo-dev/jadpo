-- the retention of `Fine`: after setting 2, store main
DELETE FROM "fine"
WHERE "id" IN (
  SELECT "id"
  FROM "fine"
  WHERE "issued_at" < $1
  ORDER BY "issued_at" ASC, "id" ASC
  LIMIT 500
  FOR UPDATE SKIP LOCKED
)
RETURNING "id"
-- $1: Cutoff as Timestamp
