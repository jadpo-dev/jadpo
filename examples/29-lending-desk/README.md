# 29 · A lending desk

A town's lending desk lends books to its customers for a fortnight and
reminds them before each is due back. Customers sign in with a link
emailed to them; the staff sign in with their Google accounts. Staff
hold one of two roles: an admin adds books and sees who gave each one,
and the front desk lends and takes books back and sees where each is
shelved. A customer sees the catalogue and their own loans, and
nothing a member of staff sees. A staff role counts only for someone
signed in through the desk's Google accounts, so switching an account
off at Google ends that person's staff access.

It is the example to start from for an app with customers and staff
in one place. Everything in it is Jadpo: there is no gateway, no
script and no code beside the app. Google's sign-in runs through
routes Jadpo writes, and the tests sign staff in through them with a
local issuer standing in for Google.

## What it teaches

1. **Two ways in, one principal.** Customers and staff are both a `Patron`, found by `Patron.email`; `active:` refuses someone switched off on their next request. See `app.jadpo` and `domains/desk/entities/patron.jadpo`.
2. **Customers by magic link.** `sign_in: { magic_link: }` writes the routes under `/auth/magic-link` and sends the link and code itself. See `authentication.jadpo` and [area 8, Magic links](../../docs/language/08-identity.md#magic-links).
3. **Staff by Google, the desk's accounts only.** Google is a connection on `Driver.oidc`, its issuer, client ID and secret settings; `providers: [google]` writes `POST /auth/google/start` and `GET /auth/google/callback`. Jadpo runs the code flow with PKCE, checks the ID token and starts the same browser session. `organisation:` keeps it to the desk's Google Workspace domain, by the `hd` Google names each account with. See `app.jadpo`, `config.jadpo`, [area 8, Providers](../../docs/language/08-identity.md#providers) and [Company accounts](../../docs/language/08-identity.md#company-accounts).
4. **One first sign-in for both.** `on_first_sign_in:` makes a customer of a new address from a magic link, and links a Google account to the member of staff taken on under its address, only once Google has verified it. A Google account that is no one's, or a customer's, is refused: staff are taken on, never made by signing in. See `domains/desk/actions/meet_newcomer.jadpo` and [area 8, A person the app has not seen](../../docs/language/08-identity.md#a-person-the-app-has-not-seen).
5. **Two roles across the whole app, only through Google.** `StaffMember` is a membership with `scope: MembershipScope.application`, its role a `StaffRole`; removing it takes the role away on the next request. Its `requires:` lets each role count only for a session started through Google, an admin's only with a second factor; signed in by link, a member of staff is a customer like any other, and a Google session lasts a day. See `domains/desk/entities/staff_member.jadpo`, `domains/desk/types/staff_role.jadpo` and [area 9, Roles that need a way in](../../docs/language/09-policy.md#roles-that-need-a-way-in).
6. **Fields only some staff read.** `Book.shelf` is for both roles and `Book.donor` for admins; everyone signed in reads the rest. See `domains/desk/entities/book.jadpo` and [area 9, Field restrictions](../../docs/language/09-policy.md#field-restrictions).
7. **A listed query beside a catalogue.** `all_books` shows what every customer may read, unlisted. `stock` and `donations` show restricted fields, so `policy.jadpo` lists each for the staff who may read them: the line is checked as the query starts, and its fields are proved against that line alone. See `policy.jadpo`, `domains/desk/queries/stock.jadpo`, `domains/desk/queries/all_books.jadpo` and [area 9, Listing a query](../../docs/language/09-policy.md#listing-a-query).
8. **Staff from a seed.** `database/seeds/first_staff.jadpo` takes on an admin and a member of the front desk through `take_on`, with no caller, so without policy: that is how the first admin exists. See `database/seeds/first_staff.jadpo`, `domains/desk/actions/take_on.jadpo` and [area 5, Seeds](../../docs/language/05-data.md#seeds).
9. **A reminder schedule over a batch.** `due_reminders` runs `remind_due` every hour, which starts a batch over every loan due within two days whose borrower hasn't been reminded; each loan's `remind_borrower` emails them as the system and marks the loan. See `domains/desk/schedules/due_reminders.jadpo`, `domains/desk/actions/remind_due.jadpo`, `domains/desk/actions/remind_borrower.jadpo` and [area 12, Batches](../../docs/language/12-jobs-and-events.md#batches-work-over-many-items).
10. **Signing in in tests.** A customer's test asks for a link and posts the token from `email.sent`; a member of staff's signs in with `test_providers.sign_in` and a `TestAccount`, through the real provider routes, its `code` the error the callback names, and its `organisation:` the domain Google names the account with. See `domains/desk/actions/meet_newcomer.test.jadpo` and [area 8, Providers](../../docs/language/08-identity.md#providers).
11. **Each role's routes, tested.** Route tests act as a person with `as:`, and say how they signed in with `sign_in:` and `strength:`: the front desk sees the stock but not the donations and can't add a book, a customer is refused the stock, a role taken away is refused on the next request, a member of staff signed in by link is refused the stock, an admin with one factor the donations, and a Google session a day after it started. A person signs in once per test, so their later requests signed in the same way send the same session, as a browser keeps its cookie. See `domains/desk/routes/staff.test.jadpo`, `domains/desk/routes/books.test.jadpo`, `domains/desk/routes/me.test.jadpo` and [area 13, `as:`](../../docs/language/13-testing.md#signing-in-for-a-route-test).
12. **The batch tested to its end.** A schedule test runs the schedule as the tick would, and the batch it starts runs to its end, two loans a page with a restart between pages, before the test's next line. See `domains/desk/schedules/due_reminders.test.jadpo` and [area 13, Batches](../../docs/language/13-testing.md#batches).
13. **Its records, and an acceptance.** `access.jadpo_record` says who can do what, how each staff role must be signed in to among it, `network.jadpo_record` the outside hosts it reaches (Google's issuer and the SMTP server), and `api/main.jadpo_record` its routes, generated ones included. A new project needs no acceptance; a later change that widens access, adds a host or changes how a role must be signed in to does. Requiring Google for the staff roles was such a change, accepted in `acceptances/20261031T1300_staff_only_through_google.jadpo`. See `access.jadpo_record`, `network.jadpo_record` and [area 18, Acceptances](../../docs/language/18-records-and-manifest.md#acceptances).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/29-lending-desk
target/release/jadpo test examples/29-lending-desk
target/release/jadpo fmt --check examples/29-lending-desk
```

- `check` ends with `checked: 58 files` and no errors.
- `test` needs PostgreSQL, and ends with `tested: 19 passed, 17 route tests, 2 schedule tests`.

To try it by hand, run `jadpo dev` in this folder, then
`jadpo seed first_staff` beside it. Sign in as staff through the
account chooser `jadpo dev` shows on its `sign-in` line, adding
`bo@lending.example` or `ann@lending.example` marked verified; the
chooser's accounts are the desk's own, with a second factor. The
customers' links land in the dev inbox.

## Try next

- In `policy.jadpo`, remove the line for `desk.stock`. `jadpo check` reports `policy.output_field_unproved` at `stock`: anyone signed in may now run it, and a customer may not read `shelf`. It reports `record.out_of_date` too, since who can do what changed.
  <!-- try {"file": "policy.jadpo", "replace": "        desk.stock: [StaffRole.admin, StaffRole.front_desk]\n", "with": "", "check": ["policy.output_field_unproved", "record.out_of_date"]} -->
- In `meet_newcomer.jadpo`, stop checking that Google verified the address: remove `or not first.address_verified`. `jadpo test` fails the test whose Google account claims Bo's address unverified, which now signs in as him.
  <!-- try {"file": "domains/desk/actions/meet_newcomer.jadpo", "replace": " or not first.address_verified", "with": "", "test": ["test.assertion_failed"], "failed": 1} -->
- In `remind_due.jadpo`, remind a day before in place of two: `clock.now + 1d`. Both schedule tests fail, since the reminders they wait for don't come.
  <!-- try {"file": "domains/desk/actions/remind_due.jadpo", "replace": "clock.now + 2d", "with": "clock.now + 1d", "test": ["test.assertion_failed"], "failed": 2} -->
- That is the last example so far. [The closest example for each feature](../README.md#the-closest-example-for-each-feature) finds the one to start from, and the [language pages](../../docs/language/README.md) hold the full specification.
