-- 20261201T0900_record_outbox_encoding: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
ALTER TABLE "jadpo_outbox" ADD COLUMN "encoding" smallint;
