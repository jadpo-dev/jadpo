-- Cake.get: one, store main
SELECT "id", "price_amount", "price_currency", "deposit_amount", "deposit_currency", "month_year", "month_month", "tags", (FALSE) IS TRUE AS "jadpo_permitted"
FROM "cake"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, compound { ty: "money", may_be_none: false }, part, compound { ty: "money", may_be_none: true }, part, compound { ty: "yearmonth", may_be_none: false }, part, json(0), bool
-- the caller may read a row: column 8
-- record Cake: id = column 0, price = column 1, deposit = column 3, month = column 5, tags = column 7
-- row versions of cake, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "cake"
--   WHERE "id" = ANY($1)
