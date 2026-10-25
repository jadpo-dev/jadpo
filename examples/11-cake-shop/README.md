# 11 · A cake shop online

A bakery that takes cake orders over HTTP: customers browse the menu and
order on the website, the phone app does the same with a token, and each
outlet's counter signs in with a key to see the orders it hands over. It
teaches routes, the ways a caller signs in, and the simplest grants:
who may read the menu, and whose orders are whose.

A route is a thin door: it says which method and path, what comes in,
what goes out and what runs, and the logic stays in the queries and
actions. Every route needs someone signed in unless it says otherwise,
and code never sees a cookie or a token, only a typed caller. Policy is
always on: each entity lists who may create, read, update and delete
it, and the compiler adds that to every statement, so no query filters
by customer by hand.

## What it teaches

1. **Three APIs and the principal.** `apis:` gives the website, the phone app and the outlets' counters each a prefix, a record and the one way to sign in it accepts; `principal` says a user is a customer and a service is an outlet's counter, each refused once its `active:` condition stops holding. See `app.jadpo`, [area 7, APIs and versions](../../docs/language/07-http.md#apis-and-versions) and [area 8, The principal](../../docs/language/08-identity.md#the-principal).
2. **Ways to sign in.** A browser session's cookie, the app's API token and an outlet's key, each kept only as a digest. See `authentication.jadpo` and [area 8, Ways to sign in](../../docs/language/08-identity.md#ways-to-sign-in).
3. **Grants.** `Access.public` lets anyone read the menu; `read: [id]` on the user entity is the customer themselves; `customer_id` and `outlet_id` on an order grant the customer and the outlet they point at. See `domains/bakery/entities/` and [area 9, Grants](../../docs/language/09-policy.md#grants).
4. **A route group.** A prefix that names the file, a route anyone may call (`authentication: RouteAuthentication.none`), the query string as a `GET`'s input, with a list field, a typed placeholder, and `///` comments that become OpenAPI's descriptions. See `domains/bakery/routes/cakes.jadpo`, `domains/bakery/types/menu_filter.jadpo` and [area 7, Route groups](../../docs/language/07-http.md#route-groups).
5. **Signing up and out.** `sessions.start` and `sessions.end` in actions, and routes that answer 204 with no output. See `domains/bakery/actions/sign_up.jadpo`, `domains/bakery/routes/account.jadpo` and [area 8, Starting and ending a session](../../docs/language/08-identity.md#starting-and-ending-a-session).
6. **The caller as a parameter.** A route passes `current_principal.user` to `place_order`, which sets the order's `customer_id` from it; a query that names no caller still gets only the caller's orders. See `domains/bakery/routes/account_orders.jadpo`, `domains/bakery/actions/place_order.jadpo`, `domains/bakery/queries/my_orders.jadpo` and [area 8, Who is calling](../../docs/language/08-identity.md#who-is-calling-current_principal).
7. **Statuses from failures.** `SuccessStatus.created` and `.no_content` on the route; a 404, 409 or 422 from each failure's kind, never written on a route; another customer's order reads as missing. See `domains/bakery/routes/account_orders.jadpo`, `domains/bakery/actions/change_order.jadpo` and [area 7, Output and status](../../docs/language/07-http.md#output-and-status).
8. **A header, a deadline and idempotent writes.** The app sends its version in `X-App-Version`; its API refuses a write without an `Idempotency-Key`, and its order route stops after 5 seconds. A route test sends a fresh key with every write, as the app does, and `idempotency_key:` sends one the test chooses, so an order sent twice gets the first answer back. See `domains/bakery/actions/place_order.jadpo`, `domains/bakery/routes/app_orders.jadpo`, `domains/bakery/routes/app_orders.test.jadpo`, [area 7, Time limits](../../docs/language/07-http.md#time-limits) and [area 13, Idempotency keys](../../docs/language/13-testing.md#idempotency-keys).
9. **API tokens.** The website gives the phone app a token, shown once; a `GET` lists the customer's tokens with `sessions.tokens`, and a `DELETE` revokes one. See `domains/bakery/actions/link_phone.jadpo`, `domains/bakery/routes/devices.jadpo` and [area 8, API tokens](../../docs/language/08-identity.md#api-tokens).
10. **Service keys.** An outlet's counter lists its keys with `Outlet.keys`, rotates them with `Outlet.issue_key` behind `authentication: RouteAuthentication.fresh`, and sees only the orders collected there. See `domains/bakery/routes/counter.jadpo`, `domains/bakery/actions/issue_outlet_key.jadpo` and [area 8, Service credentials](../../docs/language/08-identity.md#service-credentials).
11. **The API records.** `jadpo api update` writes each API's routes, fields and failure codes to `api/`, and `jadpo check` fails while a record disagrees with the routes, so every change to what clients see is a diff of one file. See `api/main.jadpo_record`, in the record format of [area 18](../../docs/language/18-records-and-manifest.md#what-is-committed-and-why), and [area 7, The public API record](../../docs/language/07-http.md#the-public-api-record).
12. **Route tests.** `request { }` sends a request through the real boundary and gives a `Response`: `as:` signs in through the route's API's own method, `params:` fills the placeholders, and the tests show a 401, a refused suspended customer, policy hiding another customer's order and an outlet's counter refused once it closes. See `domains/bakery/routes/account_orders.test.jadpo`, `domains/bakery/routes/counter.test.jadpo` and [area 13, Kinds of test](../../docs/language/13-testing.md#kinds-of-test).
13. **A checked quote.** `quote_parcel` adds a `Mass` in grams, kilograms and pounds, scales money by `* 120%` for VAT, and takes a `ParcelLabel` that can't exist invalid; it reads the order through policy, so another customer's order is `OrderNotFound`. See `domains/bakery/actions/quote_parcel.jadpo`, `domains/bakery/types/parcel_label.jadpo`, `domains/bakery/routes/account_orders.jadpo` and [area 2, What the standard types do](../../docs/language/02-values-and-types.md#what-the-standard-types-do).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/11-cake-shop
target/release/jadpo test examples/11-cake-shop
target/release/jadpo test examples/11-cake-shop --kind route
target/release/jadpo fmt --check examples/11-cake-shop
cd examples/11-cake-shop && ../../target/release/jadpo routes
../../target/release/jadpo dev
```

- `check` ends with `checked: <n> files`. It also writes an OpenAPI
  document per API to `build/openapi/` and the permissions report to
  `build/report/permissions.md`.
- `test` ends with `tested: 19 passed, 3 action tests, 16 route tests`;
  `--kind route` runs only the route tests.
- `routes` lists every route by API, with what it runs, whether it needs
  someone signed in, and the file and line it is written on.
- `dev` serves the shop on `http://127.0.0.1:3000` with a disposable
  database, and reloads on save. `curl http://127.0.0.1:3000/cakes`
  answers with an empty page; `curl http://127.0.0.1:3000/account/orders`
  answers 401 `authentication_required`; `/health/live` and
  `/health/ready` are always there. Stop it with ctrl-c.

## Try next

- In `cakes.jadpo`, delete the first route's `authentication: RouteAuthentication.none`. Reading the menu now needs someone signed in, and the menu's route test fails with `test.assertion_failed`: it gets 401, not 200.
  <!-- try {"file": "domains/bakery/routes/cakes.jadpo", "replace": "    GET / {\n        authentication: RouteAuthentication.none\n", "with": "    GET / {\n", "test": ["test.assertion_failed"], "failed": 1} -->
- In `cake.jadpo`, change `read: [Access.public]` to `read: [Access.authenticated]`. `jadpo check` reports `policy.public_grant_missing`: the menu's routes let anyone call them, and only someone signed in may read a cake.
  <!-- try {"file": "domains/bakery/entities/cake.jadpo", "replace": "read: [Access.public]", "with": "read: [Access.authenticated]", "check": ["policy.public_grant_missing"]} -->
- In `order.jadpo`, change `read: [customer_id, outlet_id]` to `read: [customer_id]`. The counter's test fails with `test.assertion_failed`: an outlet's counter can no longer read the orders it hands over, so its list is empty.
  <!-- try {"file": "domains/bakery/entities/order.jadpo", "replace": "read: [customer_id, outlet_id]", "with": "read: [customer_id]", "test": ["test.assertion_failed"], "failed": 1} -->
- In `place_order.jadpo`, read the app's version from `"Authorization"` instead of `"X-App-Version"`. `jadpo check` reports `route.header_credential`: authentication owns that header, so no input can read it. It also reports `record.out_of_date`, since the records name the header a client sends.
  <!-- try {"file": "domains/bakery/actions/place_order.jadpo", "replace": "header: \"X-App-Version\"", "with": "header: \"Authorization\"", "check": ["record.out_of_date", "route.header_credential"]} -->
- In `devices.jadpo`, change `GET / {` to `PATCH / {`. `jadpo check` reports `route.run_kind`: only a `GET` runs a query or a listing, so a read can never write. The tests that send `GET /devices` report `semantic.request_route_unknown`, since no route answers it now.
  <!-- try {"file": "domains/bakery/routes/devices.jadpo", "replace": "    GET / {", "with": "    PATCH / {", "check": ["route.run_kind", "semantic.request_route_unknown"]} -->
- Then read [12-recipe-book](../12-recipe-book/README.md), where many bakeries share one application.
