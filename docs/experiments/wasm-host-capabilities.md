# Wasm host capabilities: Cloudflare Workers and Bun

Documentation review: 30 September 2026. This is the initial HOST-1 inventory
for the [Wasm follow-up plan](wasm-large-row-plan.md), not a new hosted test result.
Platform support does not mean Jadpo has implemented or verified its adapter.

## Separate the target from the host

Wasm application code invokes capabilities supplied by its host. The same core
could use a Bun adapter on a server or a Workers adapter on Cloudflare. Workers
does not expose a complete operating system; its limits also apply to JavaScript
Workers. Choosing Wasm need not restrict every Jadpo deployment to the Workers
capability set. Cloudflare Containers are a separate deployment option.

Workers supports precompiled Wasm, SIMD, and JavaScript host bindings, but not
threads. Its WASI support is experimental and partial. Therefore compiling an
existing Rust filesystem/network library does not automatically make its system
calls work. Our adapter must explicitly expose the required operations.
[Cloudflare Wasm documentation](https://developers.cloudflare.com/workers/runtime-apis/webassembly/)

## Initial capability matrix

Bun below means Bun on a machine/container with the necessary OS permissions and
resources, not every service that happens to host Bun. Its platform APIs include
files, child processes, threads, TCP/UDP and SQLite; this is not a claim that our
pinned experimental Bun version implements every current API addition.
[Bun APIs](https://bun.com/docs/runtime/bun-apis)

| Capability | Workers host | Bun/server comparison and Jadpo consequence |
| --- | --- | --- |
| Temporary files | Writable `/tmp`, in memory, private to each request; bundle files are read-only | Server files can use real disk. Expose bounded request scratch space separately from durable files. [Filesystem](https://developers.cloudflare.com/workers/runtime-apis/nodejs/fs/) |
| Persistent local files | No persistent writable ordinary filesystem through the Workers VFS | Bun can open disk files; their durability depends on the deployment. Use explicit object/database storage on Workers, not `/tmp`. [Filesystem](https://developers.cloudflare.com/workers/runtime-apis/nodejs/fs/) |
| HTTP, streaming, WebSockets, crypto | Host APIs available | Needs adapters and lifecycle/limit tests; not intrinsically excluded by Wasm. [Runtime APIs](https://developers.cloudflare.com/workers/runtime-apis/) |
| Outbound TCP | Supported with destination/port restrictions; sockets belong to invocation context | Remote SQL connections are possible, but a normal long-lived server connection pool is not automatically portable. [TCP sockets](https://developers.cloudflare.com/workers/runtime-apis/tcp-sockets/) |
| Arbitrary inbound TCP and UDP sockets | No inbound TCP listener; `node:dgram` is a stub | Bun exposes TCP listeners and UDP. Reject requirements or use another host/service. [TCP](https://developers.cloudflare.com/workers/runtime-apis/tcp-sockets/), [compatibility flags](https://developers.cloudflare.com/workers/configuration/compatibility-flags/#enable-nodedgram-module) |
| CPU threads | No threads inside a Worker; SIMD supported | Bun has worker threads. Async I/O and distributed requests are different from shared-memory CPU parallelism. [Workers Wasm](https://developers.cloudflare.com/workers/runtime-apis/webassembly/), [Bun workers](https://bun.com/docs/runtime/workers) |
| Shell commands/native executables | `node:child_process` is a stub, not a process launcher | Bun can spawn installed programs. A Wasm port or remote/container execution is a separate implementation, not transparent subprocess support. [Flags](https://developers.cloudflare.com/workers/configuration/compatibility-flags/#enable-nodechild_process-module), [Bun spawn](https://bun.com/docs/runtime/child-process) |
| Native shared libraries | No ordinary OS library-loading capability exposed for the Jadpo Workers adapter | Bun supports Node-API/native FFI; WASI is not a native-library loader. A compatible library must be compiled into Wasm or exposed by another service. [Workers API surface](https://developers.cloudflare.com/workers/runtime-apis/), [Bun FFI](https://bun.com/docs/runtime/ffi) |
| Embedded persistent SQL | Durable Objects expose managed SQLite; D1 is another managed SQL product. Workers `node:sqlite` is a stub | Not the same interface as opening a local SQLite file. Qualify each authority and transaction API independently. [DO SQLite](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/), [storage choices](https://developers.cloudflare.com/workers/platform/storage-options/), [flags](https://developers.cloudflare.com/workers/configuration/compatibility-flags/#enable-nodesqlite-module) |
| Work after returning HTTP | `waitUntil` extends execution up to 30 seconds after response/disconnect | Not an indefinitely running daemon. Use durable jobs/queues/workflows when required, with their own retry/ordering semantics. [Context](https://developers.cloudflare.com/workers/runtime-apis/context/), [Workflows](https://developers.cloudflare.com/workflows/) |
| Memory and CPU | 128 MB per isolate, shared by JS, Wasm and concurrent requests; paid HTTP CPU default 30 s, configurable up to 5 min | Server resources are deployment choices. Stream large files; account for host buffers, scratch files and all pooled instances. CPU limits are not HTTP wall-time limits. [Limits](https://developers.cloudflare.com/workers/platform/limits/) |
| Runtime code compilation | Ordinary Worker APIs prohibit compiling arbitrary Wasm bytes at request time; deployed precompiled modules are supported | Runtime plugins/JIT-style generation would need a separate supported execution design. [Web standards](https://developers.cloudflare.com/workers/runtime-apis/web-standards/) |

Import success is not capability evidence: several Node compatibility modules are
stubs. Pin the compatibility date/flags and exercise the operation itself. Current
Node compatibility defaults differ from older deployments; this review does not
change any existing experimental configuration.
[Node compatibility](https://developers.cloudflare.com/workers/runtime-apis/nodejs/)

## Implications for the runtime

Temporary files are useful for libraries expecting filenames, but writing a large
file to `/tmp` does not spill data out of memory. A Wasm guest needs a filesystem
adapter; native `std::fs` calls on our current target are not sufficient. The
portable contract should promise request-scoped, bounded scratch storage only.
[Workers filesystem](https://developers.cloudflare.com/workers/runtime-apis/nodejs/fs/)

Putting SQLite, a queue or a KV implementation inside Wasm can supply algorithms
and in-memory data structures. It cannot by itself supply durable storage, global
coordination or recovery across Workers instances. These remain host capabilities.
On Cloudflare, choose the appropriate managed authority and prove its consistency
and failure behaviour. Do not treat all storage bindings as interchangeable.
[Storage products](https://developers.cloudflare.com/workers/platform/storage-options/)

Typed internal integers do not remove host conversion risks. Durable Object SQL
documents precision limitations when returning large integers through JavaScript
numbers and disallows raw `BEGIN`/`SAVEPOINT` through `sql.exec`. Its transaction
APIs must be assessed against Jadpo's exact contracts. Do not assume a typed Wasm
interface alone fixes numeric fidelity or nested transaction semantics.
[DO SQLite API](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/)

For full Linux filesystem access, installed binaries or multiple CPU cores,
Cloudflare Containers is an alternative host profile. It has its own lifecycle,
storage, startup and cost properties; it is not equivalent to ordinary Workers.
[Cloudflare Containers](https://developers.cloudflare.com/containers/)

## HOST-2 — Required capability evidence before adoption

Create a versioned host manifest recording the target, compatibility date/flags,
configured limits, bindings, implemented adapter operations and evidence status.
Use `documented`, `adapter implemented`, `host verified`, `unsupported`, or
`semantics unresolved`. Most rows above are currently documentation findings.

The next hosted qualification must add bounded probes for:

- temporary file write/read/delete through Wasm, concurrent/subsequent request
  isolation, rejected paths, quota handling and cleanup;
- streams and cancellation, host errors, socket restrictions and connection cleanup;
- exact integer/decimal/text transfer, nullability and malformed host values;
- required transaction/rollback semantics, authority freshness and no cross-user
  state leakage; unsupported transaction cases must reject visibly;
- isolate/guest memory accounting, pool retention and bounded scratch allocations;
- supported background-work completion/retry semantics and declared unsupported
  threads/processes/network operations (an importable stub must not pass);
- the selected precompiled module's startup and feature compatibility.

Use a separate privileged Bun/server host profile for operations unavailable in
Workers. A future build/deployment check should reject unmet capabilities before
deployment, with a clear alternative host or adapter. This proposes capability
metadata and tests, not new public syntax or silent semantic fallbacks. Actual
hosted probes/deployments remain unexecuted by this documentation change.
