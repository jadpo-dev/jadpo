-- query cakes_costing: list, store main
SELECT "id", "price_amount", "price_currency", "month_year", "month_month"
FROM "cake"
WHERE ("price_amount" = $1 AND "price_currency" = $2)
  AND ("deposit_amount" IS NULL OR ("deposit_amount" IS NULL OR "deposit_amount" >= $1))
  AND ($3 IS NULL OR NOT ("price_amount" = $4 AND "price_currency" = $5))
  AND FALSE
ORDER BY "month_year" ASC, "month_month" ASC, "id" ASC
LIMIT 20
-- $1: Part(0) of argument 0 as Int8
-- $2: Part(1) of argument 0 as Text
-- $3: Given of argument 1.price as Bool
-- $4: Part(0) of argument 1.price as Int8
-- $5: Part(1) of argument 1.price as Text
-- columns: uuid, compound { ty: "money", may_be_none: false }, part, compound { ty: "yearmonth", may_be_none: false }, part
-- record CakeView: id = column 0, price = column 1, month = column 3
-- row versions of cake, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "cake"
--   WHERE "id" = ANY($1)
