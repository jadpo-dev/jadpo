-- HelpArticle.get: one, store main
SELECT "id", "title", "shown_publicly", "body_meaning"
FROM "help_article"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, text, bool, vector
-- record HelpArticle: id = column 0, title = column 1, shown_publicly = column 2, body_meaning = column 3
-- row versions of help_article, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "help_article"
--   WHERE "id" = ANY($1)
