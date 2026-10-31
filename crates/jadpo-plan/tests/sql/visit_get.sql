-- Visit.get: one, store main
SELECT "id", "address", "spent_amount", "spent_currency", "at", "changed_at", (FALSE) IS TRUE AS "jadpo_permitted"
FROM "visit"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, text, compound { ty: "money", may_be_none: true }, part, instant, instant, bool
-- the caller may read a row: column 6
-- record Visit: id = column 0, address = column 1, spent = column 2, at = column 4, changed_at = column 5
-- row versions of visit, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "visit"
--   WHERE "id" = ANY($1)
