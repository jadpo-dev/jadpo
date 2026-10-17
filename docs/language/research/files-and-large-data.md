# Files and large data

Status: research, 2026-10-16, for
[packages and extension points](packages-and-extension-proposals.md)
(open item 9, the disk driver) and chunk 5.4 (the standard library).
Nothing here is decided. Sketches are illustrative syntax. Vendor limits
marked "verified" were read from the vendor's page on 2026-10-16; anything
marked "(to check)" has not been verified; numbers marked "estimate" are
reasoned, not measured.

Richard's questions (2026-10-16): does writing use streaming and buffers;
how are very large files handled and at what performance cost; why do most
languages offer several ways to read and write files; how are files read;
and, looking ahead, parsing and writing CSV, TSV, JSON and JSON Lines, with
writing given the same weight as reading.

## Summary

**The recommendation.** Code never chooses how bytes move. It names the
job, and the runtime streams, buffers, retries and resumes underneath.
There are four jobs, each with one form:

| Job | The one form | What the runtime does underneath |
|---|---|---|
| Take a file from a client | An upload slot (area 16, decided); a multipart slot for big files | Bytes go client to store; on the disk driver they stream to a temporary file, counted, hashed and type-checked as they arrive |
| Give a file to a client | A `File` in an `Output` becomes a signed link (decided) | The store serves it; on disk the app serves it with resumable ranges |
| Use a small file's contents | `store.read(file)` returns `Bytes`; `csv.decode`, `json.decode` turn them into typed rows | One request, buffered whole; allowed only when the field's `max_size` is within the whole-read limit (10 MB proposed) |
| Use or make a large file | A subscriber with `each:` runs its action once per piece of the file (or of a query's pages), keeping its place; `into:` writes the pieces out as one file | Ranged reads, row-by-row parsing, a cursor committed with each piece's writes, multipart writes, progress shown to operators |

**Which form applies is decided by the compiler from `max_size`.** A
`File` field up to the whole-read limit may be read whole; above it, only
in pieces. Reading a 5 GB field whole is a compile error naming `each:`.
The limit is the same on native and Workers, so a program behaves the same
on both.

**What code never sees.** File handles, paths, seeks, buffer sizes, sync or
async variants, memory mapping, line readers, chunk sizes and
decompression stacks. Files are immutable once stored, so a cursor or a
signed link never points at bytes that changed.

**Parsing and writing are typed both ways.** A file row is an `Input` when
read and an `Output` when written, exactly as an HTTP body is: the same
field rules, the same problem format (`row`, `column`, `rule`, never the
value), the same JSON encoding as the API. Headers map to fields by name or
by `column:`; odd cells use the decided `wire:` encodings. Bad rows are
collected and handed to the action, never abort the run; only file-level
faults (a missing column, an unterminated quote, undecodable text) stop
it, before any row. CSV and TSV are one format with a delimiter; JSON
Lines and top-level JSON arrays can be read and written in pieces. Writing
names who will read the file: `CsvFormat.spreadsheet` adds the byte-order
mark Excel needs and escapes formula-like text; `CsvFormat.standard` does
neither.

**Very large files.** Uploads and downloads never pass through memory: they
stream at a fixed buffer size whatever the file's size. Processing a 2 GB
CSV runs as some thousands of pieces of up to 1,000 rows each; each piece
is one transaction that commits its rows and the cursor together, so a
crash or deploy resumes at the next row, and operators see rows and bytes
done against the file's size. An export is written the same way, piece by
piece, into a multipart upload, and attached when finished.

**Performance.** Streaming costs nothing measurable over whole reads for
large sequential work and saves all the memory; for small files a whole
read is one request and simplest, which is why both exist. The real costs
are elsewhere: per-row database writes dominate imports, and an unbuffered
write per row is the classic trap a runtime removes by always buffering.
The disk driver is far quicker than S3 for the app's own reads (well under
a millisecond against tens of milliseconds per request) but serves every
download from the app server's own bandwidth and CPU, where S3 serves them
itself.

**The direct answers.** Does writing stream? Yes, wherever the size is
open-ended: uploads stream to the store, exports go out a part at a time,
downloads stream back, all through fixed buffers. Only a small file the app
makes (`store.put`) is held whole, because it is capped at the whole-read
limit. Is there a performance hit? No for throughput; the saving is memory,
and the runtime picks buffer and part sizes so code cannot get them wrong.

**Things that need checking before building.** How Workers starts each
piece in a fresh invocation (a minute's cron sweep per piece would be far
too slow); the whole-read limit and piece size, measured in the
performance spike (chunk 8.3); and the subscriber chain limit (area 12),
which `each:` replaces for long runs.

## 1. Why languages have several ways to read and write files

Every language exposes a general mechanism, a stream of bytes read or
written a buffer at a time, and then adds shortcuts for the common case,
because streams make small jobs verbose and whole-file reads make large
jobs impossible. On top of that sit knobs for performance that the library
cannot set itself because it does not know the workload, and layers left by
history: Java has three generations of file APIs (`java.io`, NIO in 2002,
NIO.2's `Files` in 2011), Node has four (callbacks, streams, promises, web
streams), and Go moved `ioutil.ReadFile` to `os.ReadFile` in 1.16 but kept
the old one.

| Way | What it is for | Examples | A job or tuning? |
|---|---|---|---|
| Whole file in one call | Small things needed all at once: a config file, an avatar, a small upload | Rust `fs::read`; Go `os.ReadFile`; Node `fs.readFile`; Python `Path.read_bytes()`; Java `Files.readAllBytes`; Kotlin `readText()`; C# `File.ReadAllBytes` | A job |
| Streaming reader or writer | Data bigger than memory, or arriving over time | Rust `Read`/`Write`; Go `io.Reader`, `io.Copy`; Node streams and `pipeline`; Python iterating a file, `shutil.copyfileobj`; Java `InputStream`; C# `Stream.CopyToAsync` | A job |
| Line or record readers | The job is records, not bytes | Rust `BufRead::lines`; Go `bufio.Scanner`; Node `readline`; Python `for line in f`; Java `Files.lines`; Kotlin `useLines`; C# `File.ReadLines` | A job, but format-specific: lines are not CSV records, since a quoted field may hold a newline |
| Random access | A part of a file without the rest: indexes, resuming, HTTP ranges, a zip's directory at its end | Rust `Seek`, `FileExt::read_at`; Go `ReadAt`, `io.SectionReader`; Java `RandomAccessFile`, `FileChannel`; C# `FileStream.Seek`, `RandomAccess`; an HTTP `Range` on S3 | A job, but rare in application code |
| Bytes or text | Decoding characters and line endings | Java `InputStream` vs `Reader`; Python `'rb'` vs `'r'` with `encoding=` and `newline=`; C# `Stream` vs `StreamReader` (which sniffs a BOM); Node `Buffer` vs `'utf8'` | A fact about the file, which must be declared, never guessed |
| Composed readers | Decompression, hashing, decoding stacked on one stream | Rust `GzDecoder<BufReader<File>>`; Go `gzip.NewReader`, `io.TeeReader`, `io.MultiWriter`; Java `GZIPInputStream`; Node `zlib.createGunzip()` in a pipeline; C# `GZipStream`; Python `gzip.open` | Plumbing: a way to combine jobs |
| Buffered or unbuffered | Avoiding a system call per small read or write | Rust `BufReader`/`BufWriter` (8 KiB default); Go `bufio`; Java `BufferedInputStream`; Python's `buffering=` | Tuning |
| Sync or async | Not blocking the program's other work while waiting | Node `readFileSync`/`readFile`/`fs/promises`; C# `ReadAllText`/`ReadAllTextAsync`; Rust `std::fs` vs `tokio::fs` (which runs blocking calls on a thread pool) | An artefact of each language's concurrency model |
| Memory mapping | Letting the operating system page a huge local file in | Rust `memmap2`; Go `golang.org/x/exp/mmap`; Java `FileChannel.map`; C# `MemoryMappedFile`; Python `mmap` | Tuning, with a hazard: a file truncated under a mapping crashes the process (`SIGBUS`); local disk only |
| Kernel copies | Moving bytes file to socket without copying through the program | Linux `sendfile`, `splice`, `copy_file_range`, which Rust's `io::copy` and Go's `io.Copy` use when they can; Java `transferTo`; C# `Socket.SendFile` | Tuning |
| Pipelines and spans | Parsing without allocating per piece | C# `System.IO.Pipelines`, `Span<T>`; Rust `bytes::Bytes` | Tuning |

**Genuinely different jobs:** having the whole thing, passing over
records once, reading a part, passing bytes through untouched, and
producing a file over time. **Tuning a language could make for the user:**
buffering, sync or async, memory mapping, kernel copies, buffer and chunk
sizes. Languages leave those to the programmer because a library function
does not know whether it is reading a 2 KB config or a 2 GB log.

The choices are also where bugs live, which matters for code agents write:
Go's `bufio.Scanner` fails on any line over 64 KiB unless given a bigger
buffer; Python's `csv` module mangles quoted newlines unless the file is
opened with `newline=''`; `readFileSync` in a Node server stalls every
request; Java's `Files.lines` leaks a handle unless closed, and its default
character set depended on the machine until JDK 18; Excel misreads UTF-8
CSV without a byte-order mark. Each is a choice the author had to get
right.

**What Jadpo can know that a library cannot:** the file's maximum size
(`max_size:` on the field), its format and row type, where it is stored,
and whether the work runs in a request or a subscriber. That is enough for
the runtime to make every tuning choice, and to pick between whole and
piecewise from the declaration.

## 2. What Jadpo needs

**Where bytes live.** In a store: a connection providing `files` (area 11):
the S3 driver (S3, R2 and other compatible services), the proposed
`Driver.disk` on one native server, local disk in development and an
in-memory store in tests. A record holds the file's key, name, size and
type. Proposed additions: the store's name, so a `File` value knows where
it lives, and a SHA-256 computed as the bytes arrive (S3's
`x-amz-checksum-sha256` on upload, computed in passing on disk), which gives
downloads an ETag and makes copies checkable.

**How code reaches them.** Three ways in, all already partly decided:

- **An upload** (area 16): a slot, a direct upload, a `FileUpload` token in
  an `Input`, checked before the write commits.
- **A file the app makes**: `attempt store.put(bytes, type)` (area 16), the
  action's one changing call; large ones through `into:` (below).
- **A `File` field read back**: `store.read(file)` for small files, `each:`
  for large ones.

**Rules from the rest of the language, and what they imply:**

- *Functions are pure.* Reading bytes is an action's work; decoding and
  encoding (`csv.decode`, `json.encode`) are pure functions over `Bytes`, so
  they run in functions and tests with no store.
- *`for` over a list is the only loop.* Nothing in user code loops until a
  file ends. The runtime drives the pieces; each piece hands the action a
  bounded `List` of rows to walk with `for`.
- *One changing outside call per action; outside calls hold no
  transaction* (areas 11 and 6). A read is not a changing call, so an action
  may read a file before its writes. Writing one export piece is the
  runtime's call, made before the piece's transaction, idempotent by part
  number.
- *One store per action.* A piece's writes go to one store, and the cursor
  commits with them (as a delivery record does today, area 12).
- *A batch keeps its progress* (area 12). A piece is that rule applied to a
  file: each piece commits on its own, and one bad piece never undoes the
  others.

**Files are immutable.** A key is written once and never overwritten;
replacing a field's file stores a new key and deletes the old one after
commit (decided). That is what makes cursors, ranged downloads and caching
safe.

**Not offered:** seeking, file handles, paths, append, memory mapping,
sync or async variants, buffer sizes, line readers and decompression
stacks. Random access exists only inside the runtime (resuming, `Range`
downloads).

**The whole-read limit.** `store.read(file)` and `Bytes` values are capped
at one limit, proposed at 10 MB (to measure in chunk 8.3), on both targets.
The compiler checks it from the field: a `File` field with `max_size:` above
the limit cannot be read whole (`files.too_large_to_read_whole`, pointing at
`each:`). A decoded whole file is also capped at 10,000 rows, the list
size beyond which piecewise work is the right shape anyway. The reason for
the number is Workers: 128 MB per isolate, shared by every request the
isolate is serving at once, and parsed rows take several times their bytes.

## 3. Very large files

### The limits that shape the design

| Limit | Value | Source |
|---|---|---|
| Workers memory | 128 MB per isolate, JavaScript heap and WebAssembly together | Verified ([Workers limits](https://developers.cloudflare.com/workers/platform/limits/)) |
| Workers CPU per HTTP request (paid) | 30 seconds by default, configurable to 5 minutes | Verified |
| Workers cron CPU (paid) | 30 seconds for intervals under an hour; 15 minutes for an hour or more | Verified |
| Workers `waitUntil` | Up to 30 seconds after the response | Verified |
| Workers cron wall time | 15 minutes | Verified |
| Workers request body | 100 MB on Free and Pro, 200 MB on Business, 5 GB self-serve on Enterprise; no response body limit | Verified |
| Workers open connections | 6 waiting for headers per invocation | Verified |
| WebAssembly memory | Grows but never shrinks; one large whole read leaves the isolate's memory high until it is recycled | Core WebAssembly has `memory.grow` and no shrink |
| R2 object | 4.995 TiB; a single `PUT` up to 5 GiB; multipart up to 10,000 parts, each at least 5 MiB and at most 5 GiB, **all the same size except the last**, which may be smaller | Verified ([R2 limits](https://developers.cloudflare.com/r2/platform/limits/), [multipart](https://developers.cloudflare.com/r2/objects/multipart-objects)) |
| S3 object | 50 TB since December 2025 | Verified ([AWS announcement](https://aws.amazon.com/about-aws/whats-new/2025/12/amazon-s3-maximum-object-size-50-tb/)) |
| S3 single `PUT` and parts | 5 GB per `PUT`; parts 5 MiB to 5 GiB, 10,000 parts, sizes may differ | (to check) |
| Native server | No platform limit; the runtime holds itself to the same limits as Workers | Proposed |
| A subscriber or schedule run | At most 15 minutes (area 12) | Decided |
| Events causing events from one request | 16 deep (area 12) | Decided |

Two consequences. A gigabyte file can only ever be streamed or processed in
pieces on Workers, and the same limits on native keep one behaviour. And
each piece must fit comfortably inside 30 seconds of CPU, since on Workers a
piece runs in `waitUntil` or a cron invocation.

### Uploading a large file

Direct to the store, as decided, so the app never holds the bytes. One
change is needed for big files: a single signed `PUT` is limited to 5 GiB,
cannot resume, and through Cloudflare's proxy is limited to 100 MB on the
Free and Pro plans. Proposed: a field whose `max_size` is above 100 MB gets
a **multipart slot** instead of a single address. The client asks the slot
for part addresses (S3 and R2: signed `UploadPart` addresses; disk:
`PUT /files/uploads/<slot>/parts/<n>`), sends parts of the size the slot
names (equal sizes, as R2 requires), and can resend any part after a
dropped connection. Attaching the `FileUpload` completes the multipart
upload and runs the same size and type checks. The shape is the same on
every store, and a generated client helper hides it.

### Downloading a large file

A signed link, as decided. S3 and R2 serve ranges themselves, so a
browser or download manager resumes a 2 GB download. On the disk driver the
app serves it (section "Disk driver" below) with single-range `Range`
requests, `If-Range` and an ETag from the file's hash, in fixed-size reads,
so a 2 GB download uses the same memory as a 2 KB one. On Workers, a file
the app passes through itself (an R2 binding read handed to a response) is
piped from the store's stream to the response without entering WebAssembly
memory.

### Processing a large file in pieces

**The shape.** A subscriber with `each:` runs its action once per piece:

```text
subscriber import_customers {
    on: CustomerImportStarted
    each: imports.rows(event.sheet, as: CustomerRow, format: CsvFormat.standard)
    run: import_customer_piece(event.import_id, piece)
    finished: finish_customer_import(event.import_id, summary)
}
```

- `piece` is a `Rows<CustomerRow>`: `rows: List<CustomerRow>`, `problems:
  List<RowProblem>` and `position` (rows read so far, bytes read, the file's
  size).
- `summary` is a `RunSummary`: rows read, rows with problems, pieces,
  started and finished times.
- `finished:` runs once after the last piece, in its own transaction.

**How a piece runs.** The runtime holds a cursor for the run: the byte
offset where the next row starts, the next row's number, and the header's
column map. For each piece it makes one ranged read from the offset (a few
MiB), parses complete rows up to the piece's size, decodes each into the
row type, collects problems, and calls the action. The action's writes and
the advanced cursor commit in one transaction, beside the delivery's own
record. A crash before commit repeats the piece from the same offset with
the same rows; nothing is written twice, because the cursor and the writes
are one commit. A deploy mid-run carries on from the cursor with the new
code. Because the runtime resumes only at row boundaries it found itself,
quoted newlines never confuse it.

**Piece size.** Up to 1,000 rows or 4 MiB of file, whichever comes first
(estimates, to measure). A thousand matches area 12's limit of 1,000 events
per action, so a piece can emit one event per row when each row needs its
own outside call. A subscriber may lower it (`piece_rows: 100`) when each
row's work is heavy; it cannot raise it.

**Pieces run one after another** within a run, because a CSV cannot be
split safely at an arbitrary byte (a quote may be open). Different runs
proceed in parallel, capped by the subscriber's concurrency.

**Failures.** A piece follows area 12's table: an outage is retried from
the same cursor; a declared failure from the action (the import was
cancelled) stops the run at that cursor as `failed`, and an operator's
retry resumes there. Bad rows are not failures; they arrive in
`piece.problems`.

**Progress.** The file's size is known, so progress is exact: the
operator's delivery view (area 14) shows rows and bytes done against the
size, pieces, problems, rate and an estimated finish. The app shows its
users whatever it records on its own import entity from `piece.position`.

**Why not chained events.** Area 11 says work over many pages runs one page
per subscriber delivery, which would mean each delivery emitting an event
for the next page. Area 12 stops chains at 16 deep, so a 10,000-piece
import would hit `event_chain_limit` at piece 16. `each:` keeps the
cursor in the run's own state instead, which also gives one progress view
and one place to retry. The same clause walks a query's pages (`each:
pages(query(...))`) and a provider's pages, which settles area 11's
pagination the same way.

**Throughput, estimated.** A 2 GB CSV of 200-byte rows is 10 million rows,
10,000 pieces. Parsing and decoding cost perhaps 50 to 200 MB/s per core
(estimate), so about 10 to 40 seconds in all; PostgreSQL inserts at
perhaps 10,000 to 50,000 rows a second when batched (estimate), so about 3
to 17 minutes; per-piece overhead (a ranged read, a transaction) adds
roughly 50 to 150 ms a piece, 8 to 25 minutes. The whole import takes
somewhere between 15 minutes and an hour, dominated by the database and
per-piece round trips.

**Workers needs one more thing (to check).** Each piece must start in a
fresh invocation with fresh CPU and `waitUntil` budgets. If the next piece
waited for the once-a-minute sweep, 10,000 pieces would take a week. The
likely mechanism is the Worker calling itself through a service binding
after each commit, with the minute sweep as the fallback; this needs a
spike.

### Writing a large file in pieces

An export uses the same subscriber, with `into:` naming where the file goes
and how it is encoded:

```text
subscriber export_orders {
    on: OrderExportRequested
    each: pages(orders_for_export(event.account_id))
    run: export_lines(piece.items)
    into: exports.csv(OrderExportLine, format: CsvFormat.spreadsheet, name: "orders.csv")
    finished: attach_order_export(event.export_id, file)
}
```

`run:` here is a pure function from a page of records to a list of export
lines, so a piece has no database writes; its only effect is the bytes. The
runtime encodes each piece's lines, writes them, and records the cursor.
When the last piece is done it completes the file and calls `finished:`
with the stored `File`.

How the bytes are written is the runtime's business:

- **S3 and R2.** One multipart upload per run. Because R2 needs equal-sized
  parts and a piece's bytes never land on a part boundary, each piece is
  first staged as its own small object keyed by run and piece number; when
  staged pieces reach the part size (16 MiB proposed, giving up to about 160
  GB per file), the runtime assembles one part from them and uploads it with
  its part number. Staging and parts are keyed, so a repeated piece rewrites
  the same object. Staged objects are deleted after completion. Cost: each
  byte is written about twice, which is cheap next to the query that
  produced it. An abandoned run's multipart upload is aborted after a day,
  like an abandoned upload (R2 also aborts incomplete multipart uploads by
  itself after seven days, to check).
- **Disk.** Appended to a temporary file. The cursor records the file's
  length after each piece; a repeated piece first truncates to the recorded
  length, then appends. Completion is fsync, rename and directory fsync.
- **A header, brackets and separators** (CSV headers, a JSON array's `[`,
  `,` and `]`) are written by the runtime around and between pieces.

**Consistency.** An export walks live data with keyset pages, so a record
changed during the export appears in whichever state its page saw. Ordering
by an immutable field (`id`, or a creation time with `id`) means no record
is skipped or repeated; the compiler warns when an `each:` query orders by
a field that can be updated.

### All-or-nothing imports

A 2 GB import cannot be one transaction. Where a team wants "all rows or
none", the recipe is two runs over the same file: the first validates only
(a pure `run:` with no writes, problems counted), and only if it finds none
does the app start the second, which writes. Or rows are written with the
import's id and made visible in one update when `finished:` runs. Both are
ordinary code; neither needs a language feature.

## 4. Performance

**Streaming against whole reads.** For sequential work the throughput is
the same: both are limited by the disk or the network, and the extra cost
of handling a stream in pieces is a few allocations per buffer, nothing
measurable next to I/O (estimate). What differs is memory: a whole read
needs the file's size, plus several times that once parsed; streaming needs
one buffer whatever the size. For a small file, a whole read is one request
and one allocation, and streaming saves nothing, so the whole read is
simpler and no slower. That is the real reason both exist, and why Jadpo
picks between them by size rather than offering both.

**Buffering.** Unbuffered small writes, one system call per row, are the
one large performance trap in file I/O (a CSV writer making a system call
per field can be tens of times slower than a buffered one, estimate). The
runtime always buffers: 64 KiB to 256 KiB reads and writes on disk, 4 to 16
MiB ranged reads from S3 (AWS recommends 8 or 16 MB byte ranges, to check),
16 MiB multipart parts.

**Back-pressure.** Every stream the runtime runs is bounded: an upload to
disk reads the socket only as fast as the disk accepts it, so a slow disk
slows the client (TCP pushes back) instead of filling memory; a download
reads the file only as fast as the client takes it.

**Zero-copy.** Natively, `sendfile` cannot help downloads because TLS is
done inside the process (rustls), so every byte is encrypted in user space
anyway; the cost is roughly one core per gigabyte a second (estimate, to
check against rustls's figures). Kernel TLS could restore `sendfile` later
(to check). On Workers, the zero-copy that matters is keeping a passed-through
file's stream out of WebAssembly memory, which the runtime does by piping
the store's stream to the response in JavaScript.

**Parsing.** Typed decoding of CSV in Rust runs at perhaps 50 to 200 MB/s
per core (estimate); the `csv` crate's raw record reading is faster, and
the typed checks (constraints, enums, dates) cost more than finding the
fields. WebAssembly on Workers is somewhat slower than native (the
experiments measured about 79% of Bun on a database workload, a different
mix). Either way, parsing is rarely the bottleneck; the database is.

**Database writes.** An import's real cost. The runtime should turn a
piece's creates into one multi-row insert (or `COPY`) rather than one
statement per row (to check how area 6's writes in a loop are built). This
is invisible to code.

**Disk against S3 for the common cases** (estimates unless marked):

| Case | Disk driver | S3 or R2 |
|---|---|---|
| The app reads a small file | Well under a millisecond from the page cache, about a millisecond from NVMe | Tens of milliseconds for the first byte; AWS has quoted 100 to 200 ms for small objects (to check) |
| The app reads a large file in pieces | Local disk speed, hundreds of MB/s or more | Roughly 50 to 100 MB/s per connection (to check); a 4 MiB piece costs one round trip |
| A client uploads | Through the app: the server's bandwidth, TLS and a little CPU | Straight to the store; the app is not involved |
| A client downloads | Through the app, as above | Straight from the store |
| Several servers | Not possible: one server | Any number |
| Durability | One disk, unless the operator backs it up | Replicated by the provider |
| Cost | The disk | Storage plus requests (and egress on S3) |

So the disk driver is faster for the app's own work on files (imports,
reads, generating) and simpler to run, and slower for serving many large
downloads, because every byte passes through the one server. S3 and R2 are
the choice for heavy downloads or more than one server.

## 5. Parsing and writing CSV, TSV, JSON and JSON Lines

### Where it fits

Area 2 decides that outside data becomes Jadpo values only by being decoded
at a boundary, with every rule checked, and the packages page decides that
byte formats stay in the runtime (JSON, form-encoded, multipart, XML
later) with per-field encodings declared through `wire:` and `decode:`.
Files follow the same rules:

- **A row read from a file is an `Input`.** It has the same field rules,
  `optional:` and `default:`, and the same problem rules (`required`,
  `type`, `format`, `variant`, `unknown_field`, and every constraint), as
  an HTTP body.
- **A row written to a file is an `Output`.** `pick:` from entities works as
  for a response.
- **Encoders and decoders are pure standard-library functions** over `Bytes`,
  owned by chunk 5.4: `csv.decode`, `csv.encode`, `json.decode`,
  `json.encode`, `json.decode_lines`, `json.encode_lines`. CSV was already
  agreed as built in; this adds JSON Lines and TSV (a CSV delimiter) beside
  it.
- **The same decoders run underneath `each:`** for large files, so a row
  type and its problems are identical whether a file is read whole or in
  pieces.
- **Format settings are named types**, as area 10 declares a `TimeFormat`
  once: `type SupplierFeed = CsvFormat { delimiter: Delimiter.semicolon
  encoding: TextEncoding.windows_1252 }`, with built-in values
  `CsvFormat.standard`, `CsvFormat.spreadsheet` and `CsvFormat.tab_separated`.

### Reading CSV and TSV

- **Dialect.** RFC 4180 as written by real tools: fields separated by the
  delimiter (comma, tab or semicolon), quoted with `"`, a quote inside a
  quoted field doubled, quoted fields may hold delimiters and line breaks.
  Lines may end in CRLF or LF, and the last line may lack one. Spaces are
  part of a field, as RFC 4180 says; nothing is trimmed.
- **TSV** is the same dialect with a tab. The IANA TSV type forbids tabs
  and line breaks in fields and has no quoting, but Excel's "tab-delimited"
  files quote like CSV, so the reader accepts quotes.
- **Headers.** A header row is expected by default. Columns map to fields
  by the field's name, or by `column: "Email address"` on the field,
  matched exactly after removing a byte-order mark. A missing required
  column is a file-level failure before any row is read, naming the column.
  Extra columns are ignored by default (spreadsheets grow columns); a
  format can say `extra_columns: false`. A file without headers names
  columns by position (`position: 3`), as an option on the format.
- **Cells to values.** An empty cell is `none` for an optional field, empty
  text for a required `Text` that allows it, and a `required` problem
  otherwise. Numbers are plain (`1234.50`, no thousands separators); dates
  are ISO 8601; enums are the variant's name; `Bool` is `true` or `false`.
  Anything else uses a `wire:` encoding (`Wire.day_month_year` for
  `16/10/2026`, from the decided built-in list, extended) or a pure
  `decode:` function from the app's libraries.
- **Row types are flat.** A CSV row type may hold only single values:
  `Text`, `Int`, `Decimal`, `Bool`, `Uuid`, the time types, plain enums and
  their nullable forms. A list, an object or an enum with fields is a
  compile error naming JSON Lines. How `Money` maps to columns (two columns,
  `price` and `price_currency`, is the obvious answer) belongs to 5.4.
- **Encodings.** UTF-8 by default, a leading byte-order mark removed. UTF-16
  with a byte-order mark is recognised and converted (Excel's "Unicode
  text" export is UTF-16 tab-separated). Anything else, such as Windows-1252
  from older systems, must be declared on the format; nothing is guessed.
  Bytes that are not valid in the encoding make that row a problem
  (`encoding`), not the whole file.
- **Problems.** Each bad row becomes a `RowProblem`: the row number, the
  column's header, the field's path and the rule, never the value (area 7's
  rule, which also keeps personal data out of logs and problem tables). The
  row number is the one a person sees in a spreadsheet: the header is row
  1, the first data row is row 2. A row with several problems reports each.
- **File-level failures** stop the run before or at the place they occur:
  a missing required column, an unterminated quote (named by the line where
  it opened, since everything after it would be one field), an unsupported
  encoding, or text where the file should be text and is not (a NUL byte in
  the first 64 KiB).

### Reading JSON and JSON Lines

- **A JSON document** decodes into any type, as a request body does: the
  same field rules, exact numbers into `Int` and `Decimal` (never through a
  float), `tag` for enums with fields, problems by path (`lines.2.quantity`,
  area 7's form). Whole reads only, so up to the whole-read limit.
- **JSON Lines** (one JSON value per line, UTF-8) is the format for large
  JSON: each line is a row, decoded and reported as a CSV row is, by line
  number. A line that is not JSON is a problem for that line, not the file.
- **A large top-level JSON array** can also be read in pieces
  (`JsonFormat.array`): the runtime resumes after a complete element, where
  the parser's state is only "inside the array". Arrays nested deeper are
  not read in pieces; a feed like `{"products": [...]}` is read whole up to
  the limit, or asked for as JSON Lines.
- **Safety rules** (section 6): duplicate keys are refused (RFC 8259 leaves
  them unpredictable, which lets two parsers disagree), depth is limited,
  and a byte-order mark is ignored, as RFC 8259 permits.

### Writing CSV and TSV

- **Columns** come from the `Output` type's fields in declared order; the
  header is the field name or its `column:`. A header row is always
  written.
- **Values** are written as the table below shows. A row type for people
  can hold `PresentationText` (a date formatted in the reader's zone and
  language with `temporal.format`, area 10), so the spreadsheet shows "16
  Oct 2026, 14:30" instead of an ISO instant.
- **Quoting** is minimal, per RFC 4180: a field is quoted only when it
  holds the delimiter, a quote, CR or LF, and a quote inside is doubled.
  TSV follows the same rule rather than refusing such fields.
- **Line endings** are CRLF, as RFC 4180 and Excel expect, including after
  the last row.
- **Who reads it is named.** `CsvFormat.spreadsheet` writes a UTF-8
  byte-order mark (without it, Excel on Windows reads UTF-8 as the local
  code page and mangles accents) and escapes formula-like text (below);
  `CsvFormat.standard` writes neither, for programs. Writing requires a
  format, so the choice is visible in review.
- **Formula escaping.** A text cell that starts with `=`, `+`, `-`, `@`, a
  tab or CR is prefixed with `'`, as OWASP recommends against CSV
  injection. Only text fields are escaped; a negative `Decimal` is written
  as `-12.50` untouched, because the type says it is a number.
- **Text that looks like a number** (a phone number `01234 567890`, a long
  account number) loses leading zeros or precision when Excel opens a CSV,
  whatever the writer does. That is a limit of CSV; an XLSX writer would
  fix it and is left until an app needs it.
- **Encoding** is UTF-8 only when writing, until an app needs another.

### Writing JSON and JSON Lines

- **One encoder.** A value is written exactly as an API response writes it:
  large `Int` and every `Decimal` as strings, enums by name, enums with
  fields as objects with `tag`, sets in canonical order (area 2).
- **JSON** is one compact document, UTF-8, no byte-order mark (RFC 8259
  forbids one), no pretty-printing option.
- **JSON Lines** is one compact value per line, each ending in LF.
- **In pieces**, either can be written: JSON Lines naturally, a JSON array
  with the runtime writing `[`, the separators and `]`.

### How each type is written

| Jadpo type | CSV and TSV cell | JSON value |
|---|---|---|
| `Text` and types built on it | The text, quoted if needed, formula-escaped for spreadsheets | String |
| `Int` | `42` | Number within JavaScript's safe range, otherwise a string (area 7) |
| `Decimal` | With its scale: `12.50` | String `"12.50"` (area 2) |
| `Bool` | `true` / `false` | `true` / `false` |
| `Uuid` | Lower-case canonical form | String |
| `Instant` | `2026-10-16T13:30:00Z` (UTC, RFC 3339) | Same, as a string |
| `CalendarDate`, `Time` | `2026-10-16`, `14:30:00` | Strings |
| `Duration` | ISO 8601, `PT5M`, as JSON writes it (area 10) | String |
| Plain enum | Variant name: `open` | `"open"` |
| Enum with fields | Compile error: use JSON Lines | Object with `tag` and its fields |
| `none` | Empty cell | As an API response writes it (`null`, to confirm against area 7) |
| `List`, `Set`, `Map`, `Object` | Compile error | Array or object |
| `PresentationText` | The text | String |
| `Bytes` | Only with `wire: Wire.base64` | Base64 string with the same `wire:` |
| `Money` | Chunk 5.4 (two columns proposed) | Chunk 5.4 |

### Writing small and large

Small outputs are written whole: `csv.encode(lines, format)` returns
`Bytes`, and `attempt store.put(bytes, FileType.csv)` stores it, both under
the whole-read limit. Anything bigger, or of unknown size (every export of
a table that grows), uses `each:` with `into:`. The compiler cannot know a
query's total size, so it warns when `csv.encode` is given a list from a
query whose limit makes the result possibly larger than the whole-read
limit.

## 6. Security and safety

- **Size.** Every way in is bounded before it is read: `max_size:` on the
  field, checked by the store (signed slot) or as the bytes arrive (disk,
  which refuses with 413 at the first byte over). Whole reads are capped by
  the compiler. A row is capped (1 MiB proposed) and so is the column count
  (1,000 proposed); a longer row is a problem, and an unterminated quote is
  a file-level failure rather than one huge field.
- **Compression and archives.** Jadpo does not unpack archives: zip files
  carry the "zip slip" path problem and decompression bombs, and an app
  needing one uses a service. gzip is the one compression read (`.csv.gz`
  is common for exports), streamed, and only with a declared
  `max_unpacked_size:`; passing it is a file-level failure, so a 1 MB
  upload cannot become 100 GB. Uploads sent with a `Content-Encoding` are
  refused. Image variants (area 16) check pixel dimensions before decoding,
  for the same reason.
- **Type checks.** Binary types are checked from their first bytes
  (decided). CSV, TSV and JSON have no signature, so they are checked as
  text in the declared encoding over the first 64 KiB.
- **JSON.** Depth limited (64 proposed, to match the API's decoder),
  duplicate keys refused, strings within the row limit, numbers exact.
- **CSV injection.** Escaped by default for spreadsheet output (section 5).
- **Serving untrusted bytes.** Store-served links come from the store's
  own host. The disk driver serves from the app's own origin, so it must
  never let an uploaded HTML or SVG run there: `Content-Disposition:
  attachment` for everything except an allow-list of image types,
  `X-Content-Type-Options: nosniff`, `Content-Security-Policy: sandbox`, and
  the client's file name only ever in `filename*` (RFC 6266), never as a
  path.
- **Path safety on disk.** Keys are generated by the runtime (a UUID,
  sharded into two directory levels), never derived from names, and the
  driver refuses any key that does not match its own pattern. Files are
  opened relative to the store's root directory without following
  symbolic links; the root must not be inside the app's source or build
  folders; files are written readable by the app's user only.
- **Disk space.** A write that runs out of space fails as `storage_full`;
  the driver also refuses new uploads below a free-space floor (5% proposed)
  so imports and the database are not starved, and reports the floor as a
  monitoring threshold.
- **Personal data.** Problems never hold values. Staged export pieces and
  temporary files are deleted on completion or abandonment. An export file
  of many people's data is a `File` field on an export record, and should
  have a `purge:` lifecycle; the personal-data report lists the export.

## 7. Testing and development

- **Fixture files.** Real files sit in a domain's `fixtures/` folder
  (`fixtures/files/customers.csv`), and a test turns one into a `File` in
  the in-memory store (`fixture_file("customers.csv", store: imports)`).
  A test can also build a file from rows (`csv.encode(...)`), which keeps
  small cases readable inline.
- **Pieces are exercised in every test.** `jadpo test` runs `each:` with
  pieces of two rows and simulates a restart between every piece, so the
  resume path is tested by every import test without anyone writing a
  crash test. A test asserts on what the app recorded (rows created,
  problems stored, the import's status) and on `summary`.
- **Fakes.** The in-memory store can be told to fail a read
  (`unavailable`) or a write (`storage_full`) in a test, to check retries
  and failure handling.
- **`jadpo dev`.** Files go to local disk, as decided. With `Driver.disk`
  in production, development uses the same driver at a folder under
  `.jadpo/`, so upload slots, ranges and signed links behave exactly as in
  production. `jadpo files ls`, `show`, `get`, `put` and `copy` work on any
  store, development or production.
- **Large files.** The runtime's own test suite runs a generated multi-GB
  CSV through `each:` and `into:` on both targets, checking memory stays
  flat and a killed process resumes; app tests never need large files.

## Sketches

All Jadpo here is illustrative. `app.jadpo` entries sit inside
`application { }`.

### Reading a small file

```text
// domains/catalogue/entities/price_list.jadpo
entity PriceList {
    id: Uuid
    sheet: File {
        store: uploads
        max_size: 5mb                    // within the whole-read limit, so it may be read whole
        types: [FileType.csv]
    }
}

// domains/catalogue/types/price_row.jadpo
type PriceRow = Input {
    sku: Sku {
        column: "SKU"
    }
    price: UnitPrice {                   // Decimal { scale: 2 }
        column: "Price (GBP)"
    }
    discontinued: Bool {
        optional: true
    }
}

// domains/catalogue/actions/read_price_list.jadpo
action read_price_list(list: PriceList) fails PriceListUnreadable -> Decoded<PriceRow> {
    var bytes: Bytes = uploads.read(list.sheet)     // no declared failures; an outage is a fault
    return attempt csv.decode(bytes, as: PriceRow, format: CsvFormat.standard) {
        unreadable: PriceListUnreadable          // a missing column, an unterminated quote
    }
}
```

`Decoded<PriceRow>` holds `rows` and `problems`; the caller writes the rows
with `for` and shows the problems. Reading a field with `max_size: 5gb`
this way is `files.too_large_to_read_whole`.

### Processing a 2 GB CSV import resumably

```text
// domains/crm/entities/customer_import.jadpo
entity CustomerImport {
    id: Uuid
    sheet: File {
        store: imports
        max_size: 5gb
        types: [FileType.csv]
    }
    status: ImportStatus                 // running, finished, cancelled
    rows_read: RowCount
    rows_with_problems: RowCount
}

// domains/crm/types/customer_row.jadpo
type CustomerRow = Input {
    email: Email {
        column: "Email address"
    }
    name: CustomerName {
        column: "Full name"
    }
    joined_on: CalendarDate {
        column: "Joined"
        wire: Wire.day_month_year        // 16/10/2026
    }
    plan: Plan {                         // an enum; the cell holds its name
        optional: true
    }
}

// domains/crm/subscribers/import_customers.jadpo
subscriber import_customers {
    on: CustomerImportStarted
    each: imports.rows(event.sheet, as: CustomerRow, format: CsvFormat.standard)
    run: import_customer_piece(event.import_id, piece)
    finished: finish_customer_import(event.import_id, summary)
}

// domains/crm/actions/import_customer_piece.jadpo
action import_customer_piece(import_id: CustomerImport.id, piece: Rows<CustomerRow>)
    fails ImportNotFound, ImportCancelled -> Unit
{
    var current: CustomerImport = attempt CustomerImport.get(import_id) {
        not_found: ImportNotFound
    }
    if (current.status == ImportStatus.cancelled) {
        reject ImportCancelled           // stops the run here; nothing more is read
    }
    var emails: List<Email> = collection.map(piece.rows, row -> row.email)
    var existing: List<Email> = customer_emails_in(emails)    // one query, not one per row
    for row in piece.rows {
        if (not collection.contains(existing, row.email)) {
            Customer.create {
                set: {
                    email: row.email
                    name: row.name
                    joined_on: row.joined_on
                    plan: row.plan
                }
            }
        }
    }
    for problem in piece.problems {
        ImportProblem.create {
            set: {
                import_id: import_id
                row: problem.row                 // the spreadsheet row number
                column: problem.column           // "Joined"
                rule: problem.rule               // format
            }
        }
    }
    attempt CustomerImport.update(current) {
        set: {
            rows_read: piece.position.rows_read
            rows_with_problems: RowCount(current.rows_with_problems + collection.count(piece.problems))
        }
    }
}
```

Each piece's customers, problems, progress and the run's cursor commit
together. A crash or deploy resumes at the next uncommitted piece.
`finish_customer_import` sets the status and emits the event that emails
the person who started it.

### Exporting a large CSV

```text
// domains/ordering/types/order_export_line.jadpo
type OrderExportLine = Output {
    reference: OrderReference {
        column: "Order"
    }
    placed: PresentationText {
        column: "Placed"                 // "16 Oct 2026, 14:30", formatted for the account's zone
    }
    status: OrderStatus {
        column: "Status"
    }
    total: Pence {
        column: "Total (pence)"
    }
}

// domains/ordering/queries/orders_for_export.jadpo
query orders_for_export(account_id: Account.id, after: PageCursor?) -> Page<Order> {
    from: Order
    where: Order.account_id == account_id
    order: [Order.id asc]                // immutable, so nothing is skipped or repeated
    page: {
        size: 500
        after: after
    }
}

// domains/ordering/subscribers/export_orders.jadpo
subscriber export_orders {
    on: OrderExportRequested
    each: pages(orders_for_export(event.account_id))
    run: export_lines(piece.items, event.zone, event.locale)
    into: exports.csv(OrderExportLine, format: CsvFormat.spreadsheet, name: "orders.csv")
    finished: attach_order_export(event.export_id, file)
}

// domains/ordering/libraries/export_lines.jadpo
function export_lines(orders: List<Order>, zone: Zone, locale: Locale) -> List<OrderExportLine> {
    return collection.map(orders, order -> OrderExportLine {
        reference: order.reference
        placed: temporal.format(order.placed_at, zone: zone, locale: locale, format: TimeFormat.date_time_medium)
        status: order.status
        total: order.total
    })
}
```

The file written:

```text
\xEF\xBB\xBFOrder,Placed,Status,Total (pence)\r\n
A-1001,"16 Oct 2026, 14:30",paid,1250\r\n
A-1002,"16 Oct 2026, 14:41",refunded,-400\r\n
```

The first three bytes are the UTF-8 byte-order mark; `Placed` is quoted
because it holds a comma; `-400` is a number, so it is not formula-escaped.

### Decoding JSON into types

```text
// domains/catalogue/types/catalogue_feed.jadpo
type FeedItem = Input {
    sku: Sku
    title: ProductTitle
    price: UnitPrice                     // "12.50" in JSON
    availability: Availability           // Enum { in_stock  back_order: { due: CalendarDate }  withdrawn }
}

type CatalogueFeed = Input {
    generated_at: Instant
    items: List<FeedItem> {
        max_length: 10000
    }
}

// whole, for a feed under the whole-read limit
action read_feed(feed: SupplierFeed) fails FeedUnreadable -> CatalogueFeed {
    var bytes: Bytes = feeds.read(feed.file)
    return attempt json.decode(bytes, as: CatalogueFeed) {
        invalid: FeedUnreadable          // problems by path: items.41.availability.due, format
    }
}

// in pieces, for a large feed sent as JSON Lines
subscriber import_feed {
    on: FeedReceived
    each: feeds.rows(event.file, as: FeedItem, format: JsonFormat.lines)
    run: apply_feed_items(event.feed_id, piece)
    finished: finish_feed(event.feed_id, summary)
}
```

A JSON document is all-or-nothing: one bad item fails the decode with every
problem listed. JSON Lines, like CSV, reports problems per line and carries
on.

### Writing each format

The same two rows, written whole by each encoder:

```text
var lines: List<StockLine> = [...]       // Output { sku: Sku  quantity: Count  status: StockStatus  note: Note? }

var a: Bytes = csv.encode(lines, format: CsvFormat.standard)
var b: Bytes = csv.encode(lines, format: CsvFormat.tab_separated)
var c: Bytes = json.encode(lines)
var d: Bytes = json.encode_lines(lines)
var stored: File = attempt exports.put(a, FileType.csv)
```

```text
// a: CsvFormat.standard (no byte-order mark, CRLF, minimal quoting)
sku,quantity,status,note\r\n
SKU-1,12,in_stock,\r\n
SKU-2,0,back_order,"Due Friday, ""confirmed"""\r\n

// b: CsvFormat.tab_separated (the same rules with a tab)
sku\tquantity\tstatus\tnote\r\n
SKU-1\t12\tin_stock\t\r\n
SKU-2\t0\tback_order\t"Due Friday, ""confirmed"""\r\n

// c: JSON (one compact document)
[{"sku":"SKU-1","quantity":12,"status":"in_stock","note":null},{"sku":"SKU-2","quantity":0,"status":"back_order","note":"Due Friday, \"confirmed\""}]

// d: JSON Lines (one value per line, LF)
{"sku":"SKU-1","quantity":12,"status":"in_stock","note":null}\n
{"sku":"SKU-2","quantity":0,"status":"back_order","note":"Due Friday, \"confirmed\""}\n
```

In pieces, the same encoders sit behind `into:`: `exports.csv(...)`,
`exports.json_lines(StockLine, name: "stock.jsonl")` or
`exports.json_array(StockLine, name: "stock.json")`.

### Uploading and downloading through the disk driver

```text
// app.jadpo
connections: {
    uploads: {
        driver: Driver.disk
        path: "/var/lib/acme/uploads"
        free_space_floor: 5%
    }
}

// domains/crm/entities/customer_import.jadpo uses store: uploads, max_size: 5gb
```

The upload, as the client sees it (a multipart slot, since `max_size` is
over 100 MB):

```text
POST /imports/slot                         an app route calling CustomerImport.sheet_upload()
<- 200 { "token": "upl_…", "part_size": 16777216, "parts_url": "/files/uploads/upl_…/parts" }

PUT /files/uploads/upl_…/parts/1           16 MiB, streamed to a temporary file, counted and hashed
<- 204
PUT /files/uploads/upl_…/parts/2           the connection drops; the client resends part 2
<- 204
...
POST /imports { "sheet": "upl_…" }         the ordinary create; the runtime joins the parts,
<- 201                                     checks size and type, fsyncs, renames and attaches
```

The download, from an `Output` holding the `File`:

```text
GET /files/uploads/7f/3a/7f3a…?expires=1760624400&signature=…
Range: bytes=1073741824-
<- 206 Partial Content
   Content-Range: bytes 1073741824-2147483647/2147483648
   ETag: "sha256-…"
   Content-Disposition: attachment; filename*=UTF-8''customers.csv
   X-Content-Type-Options: nosniff
   Content-Security-Policy: sandbox
```

## Open questions for Richard

1. **One way per job, chosen from `max_size`?** Whole reads (`store.read`,
   `csv.decode`) only for fields within one whole-read limit, the same on
   every target; larger fields only through `each:`, enforced by the
   compiler. *Recommendation:* yes, with the limit at 10 MB, confirmed by
   measurement in chunk 8.3.
2. **`each:`, `into:` and `finished:` on subscribers as the one mechanism
   for long work in pieces** (file rows, query pages, provider pages), the
   cursor kept by the runtime and committed with each piece? It replaces
   chained events, which area 12's 16-deep limit breaks after 16 pages, and
   so also settles area 11's pagination. *Recommendation:* yes.
3. **Rows are `Input` when read and `Output` when written, with `column:`
   and the decided `wire:` encodings on fields?** *Recommendation:* yes,
   so files, HTTP and provider responses share one set of rules and
   problems.
4. **Bad rows collected, not aborting?** Each piece hands the action its
   rows and problems (row as a spreadsheet numbers it, column, rule, never
   the value); only file-level problems stop the run, before the first row
   where possible. *Recommendation:* yes; "all or nothing" is a two-pass
   recipe, not a language feature.
5. **Name the reader when writing CSV?** `CsvFormat.spreadsheet` (UTF-8
   byte-order mark, CRLF, formula escaping of text cells) or
   `CsvFormat.standard` (CRLF, neither), required on every write.
   *Recommendation:* yes, required, so a reviewer sees which one an export
   is for.
6. **Encodings?** Read UTF-8 (byte-order mark removed), UTF-16 with a mark,
   and declared legacy encodings such as Windows-1252; never guess; write
   UTF-8 only. *Recommendation:* yes.
7. **Compression?** gzip only, with a required `max_unpacked_size:`; no zip
   or other archives. *Recommendation:* yes.
8. **Large uploads through a multipart slot** above 100 MB, the same shape on
   S3, R2 and disk, parts resendable, with a generated client helper?
   *Recommendation:* yes, rather than tus or the IETF resumable-upload
   draft, because S3 and R2 multipart already work this way and the
   bytes keep going straight to the store.
9. **The disk driver as specified here?** Native only and one server
   (decided in principle); files immutable with generated sharded keys;
   uploads streamed to a temporary file, then fsync and rename; downloads
   served by the app with ranges, ETags, `attachment` and a sandbox policy;
   `storage_full` and a free-space floor; `jadpo files copy` between stores.
   *Recommendation:* yes.
10. **Spreadsheet numbers and dates in other conventions** (`1.234,50`,
    `10/16/2026`)? *Recommendation:* not built in at first beyond the
    `wire:` encodings for day-month-year and month-day-year dates; a pure
    `decode:` function handles anything else.

Still to check before building, none of which changes the design: how
Workers starts each piece in a fresh invocation; S3's current single-`PUT`
and part limits; R2's automatic abort of incomplete multipart uploads;
rustls throughput per core; and whether area 6's writes in a loop are sent
as one multi-row insert.
