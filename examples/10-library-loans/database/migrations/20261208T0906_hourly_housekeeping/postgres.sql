-- 20261208T0906_hourly_housekeeping: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
CREATE TABLE "jadpo_housekeeping" (
    "slot" integer NOT NULL,
    "lease_id" uuid,
    "lease_until" timestamptz(3),
    "last_cleaned_at" timestamptz(3),
    CONSTRAINT "jadpo_housekeeping_pkey" PRIMARY KEY ("slot")
);
