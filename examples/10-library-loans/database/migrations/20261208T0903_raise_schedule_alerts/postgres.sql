-- 20261208T0903_raise_schedule_alerts: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
ALTER TABLE "jadpo_schedules" ADD COLUMN "alerts" jsonb;
