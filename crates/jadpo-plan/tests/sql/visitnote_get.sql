-- VisitNote.get: one, store main
SELECT "id", "visit_id", (FALSE) IS TRUE AS "jadpo_permitted"
FROM "visit_note"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, uuid, bool
-- the caller may read a row: column 2
-- record VisitNote: id = column 0, visit_id = column 1
-- row versions of visit_note, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "visit_note"
--   WHERE "id" = ANY($1)
