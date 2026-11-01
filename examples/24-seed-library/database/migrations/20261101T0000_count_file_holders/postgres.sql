-- 20261101T0000_count_file_holders: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
CREATE TABLE "jadpo_files" (
    "store" text NOT NULL,
    "key" text NOT NULL,
    "holders" integer NOT NULL,
    "since" timestamptz(3),
    CONSTRAINT "jadpo_files_pkey" PRIMARY KEY ("store", "key")
);
CREATE INDEX "jadpo_files_holders_since_idx" ON "jadpo_files" ("holders", "since");
CREATE OR REPLACE FUNCTION "jadpo_files_held"() RETURNS trigger LANGUAGE plpgsql AS $jadpo$
DECLARE
    field_name text;
    held_before jsonb;
    held_after jsonb;
BEGIN
    FOREACH field_name IN ARRAY TG_ARGV LOOP
        held_before := NULL;
        held_after := NULL;
        IF TG_OP <> 'INSERT' THEN
            held_before := to_jsonb(OLD) -> field_name;
        END IF;
        IF TG_OP <> 'DELETE' THEN
            held_after := to_jsonb(NEW) -> field_name;
        END IF;
        IF jsonb_typeof(held_before) IS DISTINCT FROM 'object' THEN
            held_before := NULL;
        END IF;
        IF jsonb_typeof(held_after) IS DISTINCT FROM 'object' THEN
            held_after := NULL;
        END IF;
        IF held_before IS NOT NULL AND held_after IS NOT NULL
            AND held_before ->> 'store' = held_after ->> 'store'
            AND held_before ->> 'key' = held_after ->> 'key' THEN
            CONTINUE;
        END IF;
        IF held_after IS NOT NULL THEN
            INSERT INTO "jadpo_files" ("store", "key", "holders", "since")
                VALUES (held_after ->> 'store', held_after ->> 'key', 1, NULL)
                ON CONFLICT ("store", "key") DO UPDATE
                SET "holders" = "jadpo_files"."holders" + 1, "since" = NULL
                WHERE "jadpo_files"."holders" >= 0;
            IF NOT FOUND THEN
                RAISE EXCEPTION 'a File whose bytes are deleted can''t be kept in a record (%)', held_after ->> 'key';
            END IF;
        END IF;
        IF held_before IS NOT NULL THEN
            UPDATE "jadpo_files" SET "holders" = "holders" - 1, "since" = NULL
                WHERE "store" = held_before ->> 'store'
                AND "key" = held_before ->> 'key'
                AND "holders" > 0;
        END IF;
    END LOOP;
    RETURN NULL;
END
$jadpo$;
CREATE TRIGGER "jadpo_files" AFTER INSERT OR UPDATE OF "sheet" OR DELETE ON "donation_sheet" FOR EACH ROW EXECUTE FUNCTION "jadpo_files_held"('sheet');
INSERT INTO "jadpo_files" ("store", "key", "holders") SELECT "held" ->> 'store', "held" ->> 'key', count(*) FROM (SELECT "sheet" AS "held" FROM "donation_sheet") AS "kept" WHERE jsonb_typeof("held") = 'object' GROUP BY 1, 2 ON CONFLICT ("store", "key") DO UPDATE SET "holders" = "jadpo_files"."holders" + EXCLUDED."holders";
CREATE TRIGGER "jadpo_files" AFTER INSERT OR UPDATE OF "printout" OR DELETE ON "shelf_list" FOR EACH ROW EXECUTE FUNCTION "jadpo_files_held"('printout');
INSERT INTO "jadpo_files" ("store", "key", "holders") SELECT "held" ->> 'store', "held" ->> 'key', count(*) FROM (SELECT "printout" AS "held" FROM "shelf_list") AS "kept" WHERE jsonb_typeof("held") = 'object' GROUP BY 1, 2 ON CONFLICT ("store", "key") DO UPDATE SET "holders" = "jadpo_files"."holders" + EXCLUDED."holders";
