-- query courier_years: list, store main
SELECT "courier", "sent_year", "state", "deliveries", "total_weight", "total_fee", "total_fee_currency", "total_rate", "fewest_stops", round(((div(trunc(("mean_stops_sum")::numeric * power(10::numeric, scale(("mean_stops_sum")::numeric) + scale((NULLIF("mean_stops_count", 0))::numeric))), trunc((NULLIF("mean_stops_count", 0))::numeric * power(10::numeric, scale((NULLIF("mean_stops_count", 0))::numeric)))) + CASE WHEN mod(trunc(("mean_stops_sum")::numeric * power(10::numeric, scale(("mean_stops_sum")::numeric) + scale((NULLIF("mean_stops_count", 0))::numeric))), trunc((NULLIF("mean_stops_count", 0))::numeric * power(10::numeric, scale((NULLIF("mean_stops_count", 0))::numeric)))) = 0 THEN 0 WHEN 2 * abs((mod(trunc(("mean_stops_sum")::numeric * power(10::numeric, scale(("mean_stops_sum")::numeric) + scale((NULLIF("mean_stops_count", 0))::numeric))), trunc((NULLIF("mean_stops_count", 0))::numeric * power(10::numeric, scale((NULLIF("mean_stops_count", 0))::numeric)))))::numeric) >= abs((trunc((NULLIF("mean_stops_count", 0))::numeric * power(10::numeric, scale((NULLIF("mean_stops_count", 0))::numeric))))::numeric) THEN (CASE WHEN ((trunc(("mean_stops_sum")::numeric * power(10::numeric, scale(("mean_stops_sum")::numeric) + scale((NULLIF("mean_stops_count", 0))::numeric)))) < 0) = ((trunc((NULLIF("mean_stops_count", 0))::numeric * power(10::numeric, scale((NULLIF("mean_stops_count", 0))::numeric)))) < 0) THEN 1 ELSE -1 END) ELSE 0 END))::numeric(38, 0) * power(10::numeric, -scale(("mean_stops_sum")::numeric)), scale(("mean_stops_sum")::numeric)) AS "mean_stops"
FROM (
  SELECT
    "courier" AS "courier",
    (CASE WHEN extract(year FROM "sent_on") < 0 THEN extract(year FROM "sent_on") + 1 ELSE extract(year FROM "sent_on") END)::bigint AS "sent_year",
    "state" AS "state",
    count(*) AS "deliveries",
    coalesce(sum("weight"), 0.000000000) AS "total_weight",
    coalesce(sum("fee_amount"), 0)::bigint AS "total_fee",
    $1::text AS "total_fee_currency",
    coalesce(sum("rate"), 0.00) AS "total_rate",
    min("stops") AS "fewest_stops",
    round((sum("stops"))::numeric, 1) AS "mean_stops_sum",
    count("stops") AS "mean_stops_count"
  FROM "delivery"
  WHERE "sent_on" >= $2
  GROUP BY "courier", (CASE WHEN extract(year FROM "sent_on") < 0 THEN extract(year FROM "sent_on") + 1 ELSE extract(year FROM "sent_on") END)::bigint, "state"
) AS "grouped"
ORDER BY "deliveries" DESC, "courier" COLLATE "C" ASC, "sent_year" ASC, array_position(ARRAY['booked', 'sent', 'lost']::text[], ("state")::text) ASC
LIMIT 50
-- $1: literal Text("EUR") as Text
-- $2: argument 0 as Date
-- columns: text, int, enum DeliveryState, int, quantity(mass), compound { ty: "money", may_be_none: false }, part, decimal, int, decimal
-- record CourierYear: courier = column 0, sent_year = column 1, state = column 2, deliveries = column 3, total_weight = column 4, total_fee = column 5, total_rate = column 7, fewest_stops = column 8, mean_stops = column 9
