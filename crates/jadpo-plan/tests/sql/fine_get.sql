-- Fine.get: one, store main
SELECT "id", "patron_id", "loan_id", "issued_at", (FALSE) IS TRUE AS "jadpo_permitted"
FROM "fine"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, uuid, uuid, instant, bool
-- the caller may read a row: column 4
-- record Fine: id = column 0, patron_id = column 1, loan_id = column 2, issued_at = column 3
-- row versions of fine, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "fine"
--   WHERE "id" = ANY($1)
