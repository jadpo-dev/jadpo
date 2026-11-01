-- Loan.get: one, store main
SELECT "id", "patron_id", "renewal_of", "returned_at", (FALSE) IS TRUE AS "jadpo_permitted"
FROM "loan"
WHERE "id" = $1
  AND "returned_at" IS NULL
-- $1: argument 0 as Uuid
-- columns: uuid, uuid, uuid, instant, bool
-- the caller may read a row: column 4
-- record Loan: id = column 0, patron_id = column 1, renewal_of = column 2, returned_at = column 3
-- row versions of loan, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "loan"
--   WHERE "id" = ANY($1)
