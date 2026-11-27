-- 20261127T0732_add_scopes: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
ALTER TABLE "jadpo_sessions" ADD COLUMN "scopes" text;
