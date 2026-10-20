# 26 · Delivery desk

A delivery desk takes the postcode a customer types and reads it the way
the post office writes it, with an installed package that knows UK
postcodes. Beside it sits the desk's own integration with a courier's
pickup API, written in the package format, which the desk's routes call
to book pickups, one at a time or a parcel's pickup and its return
together. The courier tells the desk when it has collected, and
publishes its prices; the desk keeps both, and emails itself a receipt
through a mail provider's package. The depot's scanners report each
parcel through a queue, and a price comparison service, imported from
its OpenAPI document, quotes before anything is booked.

It teaches packages: what one is, where it lives, how the app uses it,
how the lock keeps it exactly as it was installed, how an app calls a
provider through a connection, and how that call is faked in tests and
in `jadpo dev`, so nothing reaches the courier until the desk lets it.
Then what else a package does: webhooks, its own subscribers and
events, a poll, a recipe, recordings, the capabilities it can fill
(email, and a queue through `receiving`), a second version beside the
first with its own words, an import from OpenAPI mapped to standard
types, and an action whose several changing calls are kept as steps.

A package is a folder of Jadpo laid out as `shared/` is. A library
package carries types, failures, pure functions and its words that
several apps share; a provider package describes one API. Installed
packages live in `packages/`, committed and read-only; what the app's
authors write lives with their own code, its integrations in
`shared/providers/`.

## What it teaches

1. **A package's index.** `package.jadpo` holds a `///` description, the licence and the earliest Jadpo version the package needs, and nothing else: packages have no dependencies, and files are found by their names. The folder `packages/postcodes/v1/` gives the name in code, `Postcodes.v1`. See `packages/postcodes/v1/package.jadpo` and [area 11, The package format](../../docs/language/11-integrations.md#the-package-format).
2. **What a package offers.** `public` marks a package's API; everything else is its own. Inside it names are bare and its files need no `use` lines, and everything a public declaration's signature names is public too. See `packages/postcodes/v1/types/uk_postcode.jadpo`, `packages/postcodes/v1/libraries/uk.jadpo` and [area 11, What is public](../../docs/language/11-integrations.md#what-is-public).
3. **Using a package.** `use packages/postcodes` brings the package in, and the app writes what it offers through its name: `Postcodes.v1.EnteredPostcode`, `Postcodes.v1.InvalidPostcode`, `Postcodes.v1.uk.tidy(...)`. While one version is held the name needs no version (`Postcodes.uk.tidy(...)`); the desk holds two (item 17). See `domains/intake/actions/check_postcode.jadpo` and its test.
4. **The lock.** `jadpo.lock` pins each installed package by a digest of every file in its folder, `GUIDE.md` included, and `jadpo check` refuses a package whose files no longer match it. See `jadpo.lock` and [area 11, Installing safely](../../docs/language/11-integrations.md#installing-safely).
5. **The app's own integration.** `shared/providers/swift_couriers/v1/` is the desk's own code in exactly the package format: `provider.jadpo` says how the API is reached, and `resources/` lists its operations. It needs no lock line, and an official package can replace it later. See `shared/providers/swift_couriers/v1/provider.jadpo`, `shared/providers/swift_couriers/v1/resources/pickups.jadpo` and [area 11, Where packages live](../../docs/language/11-integrations.md#where-packages-live).
6. **Calling a provider.** `app.jadpo` connects to the integration as `couriers`, naming the domains that may call it, the credential from a secret setting and which of its hosts each deployment calls; `courier_desk`'s action calls `couriers.pickups.create(...)` before any read or write, and maps the courier's `slot_taken` to the desk's own failure of the same kind. The courier spells the collection time `collectAt`, so its field says `wire_name: "collectAt"`. `POST /pickups` runs the action, which reaches no entity, so `policy.jadpo` says who may. See `app.jadpo`, `domains/courier_desk/actions/book_pickup.jadpo`, `domains/courier_desk/routes/pickups.jadpo`, `shared/providers/swift_couriers/v1/types/pickup.jadpo` and [area 11, Calling](../../docs/language/11-integrations.md#calling).
7. **Fakes in tests.** A test never reaches the courier: each call answers through the package's fake, with the operation's generated default unless the test's `fakes:` scripts its answers in order, each a `SwiftCouriers.Pickup` or one of the operation's failures by its code. The compiler checks every scripted answer against the package, and a script the calls run past, or leave answers in, fails the test. See `domains/courier_desk/actions/book_pickup.test.jadpo` and [area 13, Fakes for integrations](../../docs/language/13-testing.md#fakes-for-integrations).
8. **Calls in development.** `jadpo dev` fakes the courier too, so the desk runs with no API key: `POST /pickups` answers with the generated default, the call shows under the request's line, and `jadpo calls` lists it with what was sent and what came back. `jadpo calls script couriers.pickups.create slot_taken` makes the next booking find its slot taken, and `development: { sandbox: [pickups.create] }` on the connection would let it through to the courier's sandbox, never its live API. See [area 11, Provider calls in development](../../docs/language/11-integrations.md#provider-calls-in-development).
9. **A package's own action.** A provider package may hold `public` actions that call its own operations by their bare names, in any order, passing answers between them, but never the app's data, another connection or another action. `book_first_slot` knows which slot comes first in the courier's day; the desk calls it as `couriers.book_first_slot(...)` and maps `slot_taken` at the call as it would an operation's. Its changing calls count as steps of the desk's action. In a test it runs its real code, and only the operation it calls is faked. See `shared/providers/swift_couriers/v1/actions/book_first_slot.jadpo`, `domains/courier_desk/actions/book_first_pickup.jadpo` and [area 11, What package code may do](../../docs/language/11-integrations.md#what-package-code-may-do).
10. **Webhooks.** `provider.jadpo`'s `webhooks:` says how Swift signs what it sends (`SignatureScheme.standard_webhooks`) and where each event's id, type and time sit in the body; each event it sends is in the package's `events/`, with its `wire_name:` and its `occurred_at`. The connection's `webhooks: { secret: }` turns on `POST /webhooks/couriers`, which `jadpo routes` lists: the runtime checks the signature, drops a repeat by its id and keeps the event in the outbox. See `shared/providers/swift_couriers/v1/provider.jadpo`, `shared/providers/swift_couriers/v1/events/pickup_collected.jadpo` and [area 11, Webhooks as events](../../docs/language/11-integrations.md#webhooks-as-events).
11. **A package's subscriber and its own events.** Swift's webhook only names the pickup, so the package's own subscriber hears it, and its action, without `public`, looks the collection up through a resource the app can't call and emits the package's public `ParcelsCollected`. That reaches the desk's subscriber through the outbox, and the desk's own action keeps it: a package never touches the app's data. A test builds the package's event to run that action. See `shared/providers/swift_couriers/v1/subscribers/collection_webhook.jadpo`, `shared/providers/swift_couriers/v1/actions/announce_collection.jadpo`, `shared/providers/swift_couriers/v1/events/parcels_collected.jadpo`, `domains/courier_desk/subscribers/parcels_collected.jadpo`, `domains/courier_desk/actions/keep_collection.jadpo` and [area 11, What package code may do](../../docs/language/11-integrations.md#what-package-code-may-do).
12. **A poll, and calls at once.** Swift's price list has no webhook, so `polls:` turns it into `TariffPublished`: the tick calls `tariffs.current` while the desk subscribes to the event, and emits it only when the prices change. The connection asks every 6 hours rather than the package's daily default. `max_concurrent: 8` is how many calls Swift takes at once from each app instance, and the connection lowers it to 4; a call past it waits for a slot. See `shared/providers/swift_couriers/v1/provider.jadpo`, `shared/providers/swift_couriers/v1/events/tariff_published.jadpo`, `app.jadpo` and [area 11, Polls](../../docs/language/11-integrations.md#polls).
13. **A recipe.** Keeping each price list needs an entity, an action and a subscriber, which a package can't own, so Swift ships them as `recipes/keep_tariff/`. `jadpo package recipe swift_couriers keep_tariff --into courier_desk` copied them into the desk, and `jadpo check` showed what to fit: their `use` lines named the domain of the package's `sample/` app, where `jadpo package check` compiles each recipe, and now name `courier_desk`. Copied, they are the desk's own code. See `shared/providers/swift_couriers/v1/recipes/keep_tariff/`, `shared/providers/swift_couriers/v1/sample/app.jadpo`, `domains/courier_desk/actions/keep_tariff.jadpo` and [area 11, Recipes](../../docs/language/11-integrations.md#recipes).
14. **Recordings.** `recordings/pickups.create.json` is an answer from Swift's sandbox, each plain value a placeholder naming its type, as `jadpo package record couriers pickups.create --input '{"postcode": "N1 9GU", "slot": "am"}'` keeps it. It confirms the operation, so `jadpo check` no longer lists the call as unconfirmed, and `jadpo test` decodes it as a `recording` test, so a type that stops matching what Swift sends fails before a real answer would. See `shared/providers/swift_couriers/v1/recordings/pickups.create.json` and [area 11, Provenance and confirmation](../../docs/language/11-integrations.md#provenance-and-confirmation).
15. **A mail package as the email connection.** `packages/resend` was installed with `jadpo package add ../../packages/resend`; its `provides/email.jadpo` maps the `email` capability onto its `emails.send`. `email:` names that connection, which lists no domains since no code calls it, and `CollectionReceipt` is emitted as any email is: [27-guest-house](../27-guest-house/README.md) sends through SMTP, and only `app.jadpo` differs. A test catches the mail whole, so `email.sent` reads it whichever connection sends it. See `packages/resend/v1/provides/email.jadpo`, `app.jadpo`, `domains/courier_desk/emails/collection_receipt.jadpo`, `domains/courier_desk/actions/keep_collection.test.jadpo` and [area 11, Capabilities](../../docs/language/11-integrations.md#capabilities).
16. **A queue through `receiving`.** The depot's scanners put a message on an Amazon SQS queue for each parcel. `packages/amazon_sqs` provides `receiving` (`provides/receiving.jadpo`), and `receiving:` names the connection, where each message's id is and the one event it carries, `ParcelScanned`, which the app declares. The tick fetches the queue, keeps each message once as its event and only then takes it off the queue; one it can't read is parked for `jadpo deliveries`. See `app.jadpo`, `packages/amazon_sqs/v2012_11_05/provides/receiving.jadpo`, `domains/depot/events/parcel_scanned.jadpo`, `domains/depot/subscribers/scanned.jadpo` and [area 11, Queues](../../docs/language/11-integrations.md#queues).
17. **Two versions side by side, and a package's words.** Postcodes 2.0.0 ships its own words, a hint for the postcode field and the wording of `InvalidPostcode`, in `translations/`, as a domain does. The desk installed it beside 1.0.0 while callers move across, so every name through the package carries its version, and the root `translations/en_gb.jadpo` rewords the hint under `postcodes_v2: { }`. Once `check_postcode` moves to `v2`, `jadpo package remove` takes `v1` away and `jadpo check` asks for the versions to be dropped from the names. See `packages/postcodes/v2/translations/`, `domains/intake/actions/postcode_hint_text.jadpo`, `translations/en_gb.jadpo` and [area 11, Where packages live](../../docs/language/11-integrations.md#where-packages-live).
18. **An import from OpenAPI, mapped to standard types.** `jadpo package import openapi specs/parcel_rates.json --as ParcelRates.v1` wrote `shared/providers/parcel_rates/v1/`, naming the document and its digest in `imported:`. It read `amount` beside `currency` as one `Money`, decoded at the edge with `wire_parts:`, and listed what the document doesn't say; the review added the sandbox host, the error code and `NoRoute`, said the quote changes nothing, narrowed the price to pounds, and listed each type it went through in `reviewed:`. The recording keeps `amount` and `currency` as sent, since the decoding reads them. See `specs/parcel_rates.json`, `shared/providers/parcel_rates/v1/provider.jadpo`, `shared/providers/parcel_rates/v1/types/quote.jadpo`, `domains/courier_desk/actions/quote_parcel.jadpo` and [area 11, Mapping imported types to standard types](../../docs/language/11-integrations.md#mapping-imported-types-to-standard-types).
19. **Several changing calls.** `book_with_return` books two pickups through `book_pickup`: two changing calls, so two steps. The desk's API requires an `Idempotency-Key`, so a retry carries on under the same key: each step is recorded before it is sent and its answer after, sealed with `steps: { encryption_key: }` in the events' store, and a retry replays what was answered, so Swift never books the outbound pickup twice. A taken return slot is handled with `match`, since failing after the first booking would leave it booked. The route's `deadline:` is 20 seconds, past two calls of 3 seconds tried 3 times. `jadpo test` runs each route test again, crashing at every step's boundary; in production `jadpo deliveries show` and `retry --not-done` look after a record left unfinished, and `jadpo runs unfinished --build` checks none would be stranded by a deploy. See `domains/courier_desk/actions/book_with_return.jadpo`, `domains/courier_desk/routes/return_bookings.jadpo`, `domains/courier_desk/routes/return_bookings.test.jadpo`, `app.jadpo` and [area 12, Several changing calls in one action](../../docs/language/12-jobs-and-events.md#several-changing-calls-in-one-action).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/26-delivery-desk
target/release/jadpo test examples/26-delivery-desk
target/release/jadpo fmt --check examples/26-delivery-desk
```

- `check` ends with `checked: 103 files` and no errors: every type the
  desk's calls reach is reviewed, and every call is confirmed by a
  recording.
- `test` needs PostgreSQL, and ends with `tested: 17 passed, 13 action
  tests, 2 route tests, 2 recording tests`.

To book a pickup by hand, run `jadpo dev` in the project, then in a
second terminal, with a fresh key for each booking as the desk's API
requires:

```sh
curl -X POST localhost:3000/pickups -H 'content-type: application/json' \
  -H "Idempotency-Key: $(uuidgen)" -d '{"postcode": "N1 9GU", "slot": "am"}'
target/release/jadpo calls
```

## Try next

- In `uk_postcode.jadpo`, raise `max_length:` to 9. `jadpo check` reports `package.digest_mismatch`: an installed package is read-only, and the app changes its own code instead.
  <!-- try {"file": "packages/postcodes/v1/types/uk_postcode.jadpo", "replace": "max_length: 8", "with": "max_length: 9", "check": ["package.digest_mismatch"]} -->
- In `check_postcode.jadpo`, write `fails InvalidPostcode` without the package's name. `jadpo check` reports `semantic.unknown_name`: outside a package, its names are written through it.
  <!-- try {"file": "domains/intake/actions/check_postcode.jadpo", "replace": ") fails Postcodes.v1.InvalidPostcode ->", "with": ") fails InvalidPostcode ->", "check": ["semantic.unknown_name"]} -->
- In `app.jadpo`, let the `rates` connection's `domains:` name `intake` instead of `courier_desk`. `jadpo check` reports `connection.domain_not_listed`: only the domains a connection lists call it, so a new caller is a line a reviewer sees.
  <!-- try {"file": "app.jadpo", "replace": "package: ParcelRates.v1\n            domains: [courier_desk]", "with": "package: ParcelRates.v1\n            domains: [intake]", "check": ["connection.domain_not_listed"]} -->
- In `book_pickup.test.jadpo`, script `slot_taken` twice for the taken slot. `jadpo test` reports `test.fake_unused`: the test makes one call, and an answer no call takes is a mistake in the test or the code.
  <!-- try {"file": "domains/courier_desk/actions/book_pickup.test.jadpo", "replace": "couriers.pickups.create: [slot_taken]", "with": "couriers.pickups.create: [slot_taken, slot_taken]", "test": ["test.fake_unused"], "failed": 1} -->
- In `pickups.create.json`, record `"collectAt"` as `"tomorrow"`. `jadpo test` reports `test.recording_undecodable`: every recording is decoded by the package's types, so one they can't read fails before a real answer like it faults a booking.
  <!-- try {"file": "shared/providers/swift_couriers/v1/recordings/pickups.create.json", "replace": "\"{Instant}\"", "with": "\"tomorrow\"", "test": ["test.recording_undecodable"], "failed": 1} -->
- In `app.jadpo`, take out `steps: { encryption_key: }`. `jadpo check` reports `step.encryption_key_required`: the courier's answers are kept while a return booking's steps run, and they are kept sealed.
  <!-- try {"file": "app.jadpo", "replace": "    steps: {\n        encryption_key: config.steps_key\n    }\n", "with": "", "check": ["step.encryption_key_required"]} -->
- In `book_with_return.jadpo`, let a taken return slot `propagate`. `jadpo check` reports `step.late_failure`: the outbound pickup is booked by then, and failing would leave it booked with the desk told nothing.
  <!-- try {"file": "domains/courier_desk/actions/book_with_return.jadpo", "replace": "PickupSlotGone -> none", "with": "PickupSlotGone -> propagate", "check": ["step.late_failure"]} -->
- In `postcode_hint_text.jadpo`, write `Postcodes.translations` without its version. `jadpo check` reports `package.version_needed`: while two versions are held, a name through the package says which.
  <!-- try {"file": "domains/intake/actions/postcode_hint_text.jadpo", "replace": "Postcodes.v2.translations", "with": "Postcodes.translations", "check": ["package.version_needed"]} -->
- In `collection_webhook.jadpo`, let the package's subscriber listen for its own `ParcelsCollected`. `jadpo check` reports `subscriber.event_invalid`: a package's subscriber hears only what its webhooks bring, and what it makes of them is for the app's subscribers.
  <!-- try {"file": "shared/providers/swift_couriers/v1/subscribers/collection_webhook.jadpo", "replace": "on: PickupCollected", "with": "on: ParcelsCollected", "check": ["subscriber.event_invalid"]} -->
- Next, [27-guest-house](../27-guest-house/README.md) sends email: templates checked against their email's fields, sent once the action that emits them commits.
