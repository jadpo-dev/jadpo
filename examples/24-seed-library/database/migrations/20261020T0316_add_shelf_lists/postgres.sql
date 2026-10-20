-- 20261020T0316_add_shelf_lists: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
CREATE TABLE "shelf_list" (
    "id" uuid NOT NULL,
    "printout" jsonb,
    "list_walk" jsonb,
    "packets_listed" bigint,
    CONSTRAINT "shelf_list_pkey" PRIMARY KEY ("id")
);
