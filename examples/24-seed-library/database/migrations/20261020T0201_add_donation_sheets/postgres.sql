-- 20261020T0201_add_donation_sheets: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
CREATE TABLE "donation_sheet" (
    "id" uuid NOT NULL,
    "sheet" jsonb NOT NULL,
    "shelving" jsonb,
    "rows_read" bigint,
    "rows_refused" bigint,
    CONSTRAINT "donation_sheet_pkey" PRIMARY KEY ("id")
);
