-- Review.get: one, store main
SELECT "id", "patron_id", "signed_as", "changed_at", (FALSE) IS TRUE AS "jadpo_permitted"
FROM "review"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, uuid, text, instant, bool
-- the caller may read a row: column 4
-- record Review: id = column 0, patron_id = column 1, signed_as = column 2, changed_at = column 3
-- row versions of review, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "review"
--   WHERE "id" = ANY($1)
