# Guides
<!-- topic: guides -->

Each guide does one common job in a Jadpo app from start to finish: the
code to write, the commands to run and what they print, and the
reference topics that have the full rules. Every guide's code is
compiled and its tests run, so each one works as shown. Read the one
for the job in front of you; they don't depend on each other. To learn
Jadpo from the start, follow the [Learn path](../learn/README.md)
instead.

| Guide | You end with |
|---|---|
| [Build an API](01-build-an-api.md) | routes with typed input and output, the committed API record, and the OpenAPI document |
| [Store and change a record](02-store-and-change-a-record.md) | an entity, its migration, and actions that add, change and delete records |
| [Query records](03-query-records.md) | a query for one record, and a list a caller narrows and pages through |
| [Handle failures](04-handle-failures.md) | a failure of your own, refused with the right status, handled where a caller wants to |
| [Add sign-in](05-add-sign-in.md) | sign-in by emailed link and with Google, and routes that know who is calling |
| [Call an outside API](06-call-an-outside-api.md) | a typed description of another service, called through a connection and faked in tests |
| [Run background work](07-run-background-work.md) | a schedule that starts a batch, an event with its subscriber, and a test of the whole run |
| [Write tests](08-write-tests.md) | action and route tests, fixtures, a fixed clock and tests signed in as someone |
| [Get a person's sign-off](09-get-a-sign-off.md) | an acceptance for a change that could break someone, with a person's reason |
| [Deploy natively](10-deploy-natively.md) | the app built into one program, running against a real database |
| [Deploy to Cloudflare Workers](11-deploy-to-workers.md) | the app built as a Worker, reaching its database through Hyperdrive |
