# 26 · Delivery desk

A delivery desk takes the postcode a customer types and reads it the way
the post office writes it, with an installed package that knows UK
postcodes. Beside it sits the desk's own integration with a courier's
pickup API, written in the package format, which the desk calls to book
a pickup. It teaches packages: what one is, where it lives, how the app
uses it, how the lock keeps it exactly as it was installed, and how an
app calls a provider through a connection.

A package is a folder of Jadpo laid out as `shared/` is. A library
package carries types, failures and pure functions several apps share; a
provider package describes one API. Installed packages live in
`packages/`, committed and read-only; what the app's authors write lives
with their own code, its integrations in `shared/providers/`.

## What it teaches

1. **A package's index.** `package.jadpo` holds a `///` description, the licence and the earliest Jadpo version the package needs, and nothing else: packages have no dependencies, and files are found by their names. The folder `packages/postcodes/v1/` gives the name in code, `Postcodes.v1`. See `packages/postcodes/v1/package.jadpo` and [area 11, The package format](../../docs/language/11-integrations.md#the-package-format).
2. **What a package offers.** `public` marks a package's API; everything else is its own. Inside it names are bare and its files need no `use` lines, and everything a public declaration's signature names is public too. See `packages/postcodes/v1/types/uk_postcode.jadpo`, `packages/postcodes/v1/libraries/uk.jadpo` and [area 11, What is public](../../docs/language/11-integrations.md#what-is-public).
3. **Using a package.** `use packages/postcodes` brings the package in, and the app writes what it offers through its name: `Postcodes.EnteredPostcode`, `Postcodes.InvalidPostcode`, `Postcodes.uk.tidy(...)`. See `domains/intake/actions/check_postcode.jadpo` and its test.
4. **The lock.** `jadpo.lock` pins each installed package by a digest of every file in its folder, `GUIDE.md` included, and `jadpo check` refuses a package whose files no longer match it. See `jadpo.lock` and [area 11, Installing safely](../../docs/language/11-integrations.md#installing-safely).
5. **The app's own integration.** `shared/providers/swift_couriers/v1/` is the desk's own code in exactly the package format: `provider.jadpo` says how the API is reached, and `resources/` lists its operations. It needs no lock line, and an official package can replace it later. See `shared/providers/swift_couriers/v1/provider.jadpo`, `shared/providers/swift_couriers/v1/resources/pickups.jadpo` and [area 11, Where packages live](../../docs/language/11-integrations.md#where-packages-live).
6. **Calling a provider.** `app.jadpo` connects to the integration as `couriers`, naming the domains that may call it, the credential from a secret setting and which of its hosts each deployment calls; `courier_desk`'s action calls `couriers.pickups.create(request)` before any read or write, and maps the courier's `slot_taken` to the desk's own failure of the same kind. The courier spells the collection time `collectAt`, so its field says `wire_name: "collectAt"`. See `app.jadpo`, `domains/courier_desk/actions/book_pickup.jadpo`, `shared/providers/swift_couriers/v1/types/pickup.jadpo` and [area 11, Calling](../../docs/language/11-integrations.md#calling).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/26-delivery-desk
target/release/jadpo test examples/26-delivery-desk
target/release/jadpo fmt --check examples/26-delivery-desk
```

- `check` ends with `checked: 17 files` and no errors.
- `test` ends with `tested: 2 passed, 2 action tests`.

## Try next

- In `uk_postcode.jadpo`, raise `max_length:` to 9. `jadpo check` reports `package.digest_mismatch`: an installed package is read-only, and the app changes its own code instead.
  <!-- try {"file": "packages/postcodes/v1/types/uk_postcode.jadpo", "replace": "max_length: 8", "with": "max_length: 9", "check": ["package.digest_mismatch"]} -->
- In `check_postcode.jadpo`, write `fails InvalidPostcode` without the package's name. `jadpo check` reports `semantic.unknown_name`: outside a package, its names are written through it.
  <!-- try {"file": "domains/intake/actions/check_postcode.jadpo", "replace": ") fails Postcodes.InvalidPostcode ->", "with": ") fails InvalidPostcode ->", "check": ["semantic.unknown_name"]} -->
- In `app.jadpo`, let the connection's `domains:` name `intake` instead of `courier_desk`. `jadpo check` reports `connection.domain_not_listed`: only the domains a connection lists call it, so a new caller is a line a reviewer sees.
  <!-- try {"file": "app.jadpo", "replace": "domains: [courier_desk]", "with": "domains: [intake]", "check": ["connection.domain_not_listed"]} -->
- That is the last example so far. The [language pages](../../docs/language/README.md) hold the full specification.
