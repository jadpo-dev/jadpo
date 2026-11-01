-- LoanNote.get: one, store main
SELECT "id", "loan_id", (FALSE) IS TRUE AS "jadpo_permitted"
FROM "loan_note"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, uuid, bool
-- the caller may read a row: column 2
-- record LoanNote: id = column 0, loan_id = column 1
-- row versions of loan_note, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "loan_note"
--   WHERE "id" = ANY($1)
