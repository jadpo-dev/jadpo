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
| [Limit how often a route is called](14-limit-how-often-a-route-is-called.md) | a public form limited to five requests an hour, an API's own limit, and clients counted properly behind a proxy |
| [Store and change a record](02-store-and-change-a-record.md) | an entity, its migration, and actions that add, change and delete records |
| [Keep a record of who changed what](16-keep-a-record-of-who-changed-what.md) | an audited entity, its history read back with what each caller may see, and a route that sends it |
| [Query records](03-query-records.md) | a query for one record, and a list a caller narrows and pages through |
| [Add search to a list](15-add-search-to-a-list.md) | a searchable entity, its migration, and a route giving the best matches first, a page at a time |
| [Handle failures](04-handle-failures.md) | a failure of your own, refused with the right status, handled where a caller wants to |
| [Translate your app's text](19-translate-your-apps-text.md) | typed messages in two languages, chosen from the browser's header, with stale translations caught by `jadpo check` |
| [Add sign-in](05-add-sign-in.md) | sign-in by emailed link and with Google, and routes that know who is calling |
| [Call an outside API](06-call-an-outside-api.md) | a typed description of another service, called through a connection and faked in tests |
| [Receive a webhook](17-receive-a-webhook.md) | a sender described as a package, its signed requests checked and de-duplicated, and a subscriber keeping each event |
| [Run background work](07-run-background-work.md) | a schedule that starts a batch, an event with its subscriber, and a test of the whole run |
| [Export records to a file](18-export-records-to-a-file.md) | a query's records written to a spreadsheet-safe CSV in the background, with its run tracked on a record |
| [Write tests](08-write-tests.md) | action and route tests, fixtures, a fixed clock and tests signed in as someone |
| [Ship a change behind a flag](13-ship-a-change-behind-a-flag.md) | a change deployed switched off, both sides tested, turned on with no deploy, and the flag removed |
| [Get a person's sign-off](09-get-a-sign-off.md) | an acceptance for a change that could break someone, with a person's reason |
| [Ask a model for a typed answer](20-ask-a-model-for-a-typed-answer.md) | a checked prompt, a decoded answer with every failure named, a daily budget, and tests that never reach a model |
| [Deploy natively](10-deploy-natively.md) | the app built into one program, running against a real database |
| [Deploy to Cloudflare Workers](11-deploy-to-workers.md) | the app built as a Worker, reaching its database through Hyperdrive |
| [Send an email](12-send-an-email.md) | a welcome email with typed templates, sent over SMTP after commit, tested and read in the dev inbox |
