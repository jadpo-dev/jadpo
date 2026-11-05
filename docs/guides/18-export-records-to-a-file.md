# Export records to a file
<!-- topic: guide-export-records-to-a-file -->

An export can hold more records than fit in memory or in one request, so in Jadpo
it runs in the background. `csv.write` walks a query a page at a time and writes
each record as a row, and tells your app when the file is ready. A crash partway
through carries on where it stopped. This guide writes a bakery's price list as a
file a spreadsheet can open, on the server's own disk.

You start from an app with a `POST /loaves` route that adds a loaf, through an
action called `add_loaf`. [Build an API](01-build-an-api.md) shows how to write
one.

```jadpo check project=guide-export file=domains/bakery/types/new_loaf.jadpo hidden
use bakery/entities { Loaf }

/// What a caller sends to add a loaf.
type NewLoaf = Input {
    code: Loaf.code
    price: Loaf.price
}
```

```jadpo check project=guide-export file=domains/bakery/types/loaf_added.jadpo hidden
use bakery/entities { Loaf }

/// A loaf, once it is added.
type LoafAdded = Output {
    pick: [Loaf.id, Loaf.code]
}
```

```jadpo check project=guide-export file=domains/bakery/actions/add_loaf.jadpo hidden
use bakery/entities { Loaf }
use bakery/types { LoafAdded, NewLoaf }

/// Adds a loaf.
action add_loaf(input: NewLoaf) -> LoafAdded {
    var loaf: Loaf = Loaf.create {
        set: {
            code: input.code
            price: input.price
        }
    }
    return LoafAdded {
        id: loaf.id
        code: loaf.code
    }
}
```

```jadpo check project=guide-export file=domains/bakery/routes/loaves.jadpo hidden
use bakery/types { LoafAdded, NewLoaf }
use bakery/actions { add_loaf }

/// The bakery's loaves.
routes Loaves {
    prefix: /loaves

    /// Adds a loaf.
    POST / {
        authentication: RouteAuthentication.none
        input: NewLoaf
        output: LoafAdded
        status: SuccessStatus.created
        run: add_loaf(input)
    }
}
```

## Say where the file goes

A file goes in a store, a connection in `app.jadpo`. `Driver.disk` keeps files on
the server's own disk, under the `path:` you give, which has to be outside your
project. `jadpo dev` keeps them under `.jadpo/files/` instead, so nothing here needs
that folder on your machine.

`app.jadpo`:

```jadpo check project=guide-export file=app.jadpo
application {
    connections: {
        price_lists: {
            driver: Driver.disk
            path: "/var/lib/bakery/price-lists"
        }
    }
}
```

A native app with a disk store runs on one server, since a second one wouldn't see
the files, and the build's manifest records that. Cloudflare Workers has no disk, so
a deployable for Workers can't use this driver.

`connections:` and `Driver.disk` are built into Jadpo, and the name `price_lists` is
yours.

## Read the records a page at a time

The records to export are loaves, each a stored `Loaf`.

`domains/bakery/entities/loaf.jadpo`:

```jadpo check project=guide-export file=domains/bakery/entities/loaf.jadpo
/// A loaf the bakery sells.
entity Loaf {
    id: Uuid
    code: Text {
        min_length: 1
        max_length: 20
    }
    price: Decimal {
        scale: 2
        min: 0.00
    }

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
    }
}
```

An export reads a query that gives pages, so a large table never has to fit in
memory. Order it by something that doesn't change, here `id`, so a change
during the run can't move a record between pages. `PageSize` and `PageCursor` are
standard types for how many records a page holds and where the next one starts.

`domains/bakery/types/loaf_page.jadpo`:

```jadpo check project=guide-export file=domains/bakery/types/loaf_page.jadpo
/// Where a walk over the loaves has got to.
type LoafPage = Input {
    page_size: PageSize {
        default: 100
    }
    after: PageCursor {
        optional: true
    }
}
```

`domains/bakery/queries/loaves_by_id.jadpo`:

```jadpo check project=guide-export file=domains/bakery/queries/loaves_by_id.jadpo
use bakery/entities { Loaf }
use bakery/types { LoafPage }

/// Every loaf, a page at a time, in an order no change moves.
query loaves_by_id(page: LoafPage) -> Page<Loaf> {
    from: Loaf
    order: [Loaf.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}
```

## Say what a row holds

A row is an `Output` type. Its fields are the file's columns, in order, each named by
its `column:` or by the field's name. A field can take its type from the entity's field,
as these do.

`domains/bakery/types/price_line.jadpo`:

```jadpo check project=guide-export file=domains/bakery/types/price_line.jadpo
use bakery/entities { Loaf }

/// One line of the price list a spreadsheet reads.
type PriceLine = Output {
    code: Loaf.code {
        column: "Loaf code"
    }
    price: Loaf.price {
        column: "Price"
    }
}
```

## Keep track of each export

An entity records each price list: the file once it exists, the run writing it, and
how many loaves it lists. `File` is a field type holding a stored file. Each `File`
field names its store and what it may hold. Every write is checked against both.
A subscriber or a batch runs as the system caller, so `read:` and `update:` name
`Access.system`.

`domains/bakery/entities/price_list.jadpo`:

```jadpo check project=guide-export file=domains/bakery/entities/price_list.jadpo
/// A price list written for the shop, and how writing it went.
entity PriceList {
    id: Uuid
    list_file: File? {
        store: price_lists
        max_size: 20mb
        types: [FileType.csv]
    }
    list_run: BatchRun?
    loaves_listed: Int?

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.public, Access.system]
    }
}
```

## Start the export

`csv.write` starts the run and returns a `BatchRun` at once. The run begins after
your action commits.

- `format:` says who reads the file, and is required. `CsvFormat.spreadsheet` writes
  a byte-order mark and puts `'` before text that a spreadsheet would read as a
  formula. `CsvFormat.standard` does neither.
- `from:` is the query, and `each:` turns each of its records into a row.
- `finished:` runs when every page is written, and gets an `ExportSummary` with the
  `file` and how many `items` it holds.
- `key:` makes starting it safe to repeat: the same key never starts a second run.

`domains/bakery/actions/list_prices.jadpo`:

```jadpo check project=guide-export file=domains/bakery/actions/list_prices.jadpo
use app { price_lists }
use bakery/entities { PriceList }
use bakery/types { LoafPage, PriceLine, PriceListView }
use bakery/actions { prices_listed }
use bakery/queries { loaves_by_id }

/// Starts writing every loaf's price into a spreadsheet.
action list_prices() -> PriceListView {
    var price_list: PriceList = PriceList.create {
        set: {
            list_file: none
            list_run: none
            loaves_listed: none
        }
    }
    var list_run: BatchRun = csv.write(
        price_lists,
        name: "prices.csv",
        format: CsvFormat.spreadsheet,
        from: loaves_by_id(LoafPage {}),
        each: loaf -> PriceLine {
            code: loaf.code
            price: loaf.price
        },
        finished: done -> prices_listed(done, price_list.id),
        key: price_list.id
    )
    var started: PriceList = PriceList.update(price_list) {
        set: {
            list_run: list_run
        }
    }
    return PriceListView {
        id: started.id
        loaves_listed: started.loaves_listed
    }
}
```

`domains/bakery/actions/prices_listed.jadpo`:

```jadpo check project=guide-export file=domains/bakery/actions/prices_listed.jadpo
use bakery/entities { PriceList, PriceListNotFound }

/// Keeps the written price list, and how many loaves it lists.
action prices_listed(done: ExportSummary, list_id: PriceList.id) fails PriceListNotFound -> Unit {
    attempt PriceList.update(list_id) {
        set: {
            list_file: done.file
            loaves_listed: done.items
        }
    }
}
```

## Add the routes

One route starts an export, and one says how far it has got.

`domains/bakery/types/price_list_view.jadpo`:

```jadpo check project=guide-export file=domains/bakery/types/price_list_view.jadpo
use bakery/entities { PriceList }

/// A price list, and how far writing it has got.
type PriceListView = Output {
    pick: [PriceList.id, PriceList.loaves_listed]
}
```

`domains/bakery/queries/price_list_by_id.jadpo`:

```jadpo check project=guide-export file=domains/bakery/queries/price_list_by_id.jadpo
use bakery/entities { PriceList, PriceListNotFound }
use bakery/types { PriceListView }

/// One price list.
query price_list_by_id(list_id: PriceList.id) fails PriceListNotFound -> PriceListView {
    from: PriceList
    where: PriceList.id == list_id
    missing: PriceListNotFound
}
```

`domains/bakery/routes/price_lists.jadpo`:

```jadpo check project=guide-export file=domains/bakery/routes/price_lists.jadpo
use bakery/entities { PriceList }
use bakery/types { PriceListView }
use bakery/actions { list_prices }
use bakery/queries { price_list_by_id }

/// Price lists written for the shop.
routes PriceLists {
    prefix: /price-lists

    /// Starts writing a price list.
    POST / {
        authentication: RouteAuthentication.none
        output: PriceListView
        status: SuccessStatus.created
        run: list_prices()
    }

    /// How far a price list has got.
    GET /{list_id: PriceList.id} {
        authentication: RouteAuthentication.none
        output: PriceListView
        run: price_list_by_id(list_id)
    }
}
```

Run `jadpo migrate new add_price_lists` for the new table, and `jadpo api update` for
the routes.

## Test it

A test runs an export's whole run before its next line, so it reads the finished
record straight away.

`domains/bakery/actions/list_prices.test.jadpo`:

```jadpo check project=guide-export file=domains/bakery/actions/list_prices.test.jadpo
use bakery/entities { PriceList }
use bakery/types { LoafAdded, NewLoaf, PriceListView }
use bakery/actions { add_loaf }

test "a price list lists every loaf and keeps its file" {
    var rye: LoafAdded = add_loaf(
        NewLoaf {
            code: "rye"
            price: 3.50
        }
    )
    var spelt: LoafAdded = add_loaf(
        NewLoaf {
            code: "spelt"
            price: 4.20
        }
    )
    assert rye.code != spelt.code
    var started: PriceListView = list_prices()
    assert started.loaves_listed == none
    var finished: PriceList = attempt PriceList.get(started.id)
    assert finished.loaves_listed == 2
    assert finished.list_file != none
}
```

```sh
jadpo test
```

```text
tested: 1 passed, 1 action test
```

## Try it

Start `jadpo dev`, and add three loaves with `POST /loaves`, using the codes `rye`,
`spelt` and `=sour`. Then start an export:

```sh
curl -X POST http://localhost:3000/price-lists
```

```text
{"id":"01a1a657-d69c-7260-8930-9b94530c2894","loaves_listed":null}
```

`loaves_listed` is empty, because the response came before the run began. Ask again
with the id you got:

```sh
curl http://localhost:3000/price-lists/01a1a657-d69c-7260-8930-9b94530c2894
```

```text
{"id":"01a1a657-d69c-7260-8930-9b94530c2894","loaves_listed":"3"}
```

The file is under `.jadpo/files/`, named by a key the runtime generates:

```sh
find .jadpo/files -type f
```

```text
.jadpo/files/price_lists/01/a1/01a1a657-d69c-769d-b057-cd376d60c905
```

Open that path with your own key. It holds one line for each loaf, in the order of
their ids, each ending in CRLF, and a spreadsheet won't read `=sour` as a formula:

```text
Loaf code,Price
rye,3.50
spelt,4.20
'=sour,5.00
```

## Other formats and what comes next

`tsv.write` takes the same arguments as `csv.write`. `json_lines.write` writes one JSON
value on each line, and `json.write` one JSON list, and neither takes a `format:`.

`batches.status(run)` gives a `BatchProgress` for a run, with the items it has done,
if your page shows a progress bar.

To send the file on, give an email a `File` field, which attaches it when the email is
sent. [Send an email](12-send-an-email.md) shows how to send one.

## Read more

[Files](../language/16-built-in-services.md#files),
[Data formats](../language/19-standard-library.md#data-formats-json-csv-tsv-and-json-lines),
[Batches](../language/12-jobs-and-events.md#batches) and
[How an export writes its file](../language/16-built-in-services.md#how-an-export-writes-its-file).

## Not built yet

- A route that lets someone download a stored file isn't built yet, and neither is
  a signed link to one. Send the file as an email attachment, or read it where the
  disk store keeps it.
- The S3 driver isn't built yet, so the disk store, on one native server, is the only
  place an export can go.
- `jadpo files`, the command for listing and fetching a store's files, isn't built yet.
  Use the store's folder, as above.
- `jadpo batches`, the command for listing and cancelling runs, isn't built yet. Keep
  each run on a record, as `PriceList` does.
