-- Paper.get: one, store main
SELECT "id", "meaning", "full_meaning"
FROM "paper"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, vector, vector
-- record Paper: id = column 0, meaning = column 1, full_meaning = column 2
