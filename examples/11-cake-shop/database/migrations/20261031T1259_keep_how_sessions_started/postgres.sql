-- 20261031T1259_keep_how_sessions_started: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
ALTER TABLE "jadpo_sessions" ADD COLUMN "sign_in_method" text;
ALTER TABLE "jadpo_sessions" ADD COLUMN "strength" text;
