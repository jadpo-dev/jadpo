-- 20261019T2054_count_round_invoices: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
ALTER TABLE "rent_round" ADD COLUMN "invoiced" bigint;
ALTER TABLE "rent_round" ADD COLUMN "unlet" bigint;
