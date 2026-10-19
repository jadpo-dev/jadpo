# 13 · A lettings office's settings and keys

A lettings agency's back office: it takes deposits, sends contractors to
urgent repairs with the tenant's access note, signs its staff in,
checks the rent collector's webhooks, signs rent receipts, and lets
tenants on the council's scheme sign in with the council's own sign-in
service. It teaches settings: what differs between deployments, where
those values come from, and the keys code uses without ever reading
them.

Every value that could differ between one deployment and the next is a
setting, declared once in root `config.jadpo` and read as
`config.<name>`; a value that never differs is a `constant` in the code.
A setting arrives from an environment variable, checked against its
type before the app starts, so code never reads the environment and
never sees a setting it can't use. Secrets, such as the database's
address and the office's keys, go only to places the compiler owns: a
connection's `url:` and a `crypto` call's `key:`. Nothing the office
logs, stores, returns or compares can hold one.

## What it teaches

1. **Settings.** `config { }` in root `config.jadpo` lists every setting, in name order; each reads the variable named after it in capitals, and is required unless it has a `default:`. See `config.jadpo` and [area 10, Settings](../../docs/language/10-configuration-and-time.md#settings-configjadpo).
2. **What a setting can hold.** A named type built on `Duration`, a `Text` type, a `Zone` and an enum of the app's own, each checked when the app starts, so `config.urgent_repair_within` is a `RepairWindow` with nothing to build. See `config.jadpo`, `domains/lettings/libraries/repairs.jadpo` and [area 10, What a setting can hold](../../docs/language/10-configuration-and-time.md#what-a-setting-can-hold).
3. **Connection addresses.** `PostgresUrl` and `RedisUrl` are always secret, and a connection's `url:` is where they go. See `app.jadpo` and [area 10, Database and other connection URLs](../../docs/language/10-configuration-and-time.md#database-and-other-connection-urls).
4. **Settings in code.** An action reads `config.log_level` and `config.office_zone`; a library function reads `config.office_name`, since a setting can't change while the app runs. See `domains/lettings/actions/repair_audit_line.jadpo` and `domains/lettings/libraries/receipts.jadpo`.
5. **Constants.** The deposit cap is fixed by law, not by an operator, so it is a constant in a library, read as `deposits.cap_weeks`; a holding deposit's weeks are a constant beside the action that uses them. See `domains/lettings/libraries/deposits.jadpo`, `domains/lettings/actions/deposit_due.jadpo` and [area 10, A constant or a setting](../../docs/language/10-configuration-and-time.md#a-constant-or-a-setting).
6. **Sealing data with a key.** An `EncryptionKey` seals each access note to its tenancy, so a copy moved to another tenancy can't be opened; sealing reads the random generator, so only an action seals. See `domains/lettings/actions/seal_access_note.jadpo`, `domains/lettings/actions/contractor_visit.jadpo` and [area 19, Hashing and cryptography](../../docs/language/19-standard-library.md#hashing-and-cryptography).
7. **Passwords.** Each is stored as its Argon2id hash, and a hash made with older settings signs in and is made afresh. See `domains/lettings/actions/set_password.jadpo` and `domains/lettings/actions/staff_sign_in.jadpo`.
8. **A shared secret.** A `SigningKey` both sides hold signs and checks the rent collector's webhooks, whose bodies are read as JSON and written back as JSON. See `domains/lettings/actions/rent_paid.jadpo` and `domains/lettings/actions/acknowledge_payment.jadpo`.
9. **A private key.** A `PrivateKey` signs receipts; its public half isn't secret, and anyone can check a receipt with it. See `domains/lettings/actions/issue_receipt.jadpo`, `domains/lettings/actions/receipt_key.jadpo` and `domains/lettings/actions/check_receipt.jadpo`.
10. **Another service's tokens.** The council publishes its keys as a key set; `crypto.verify_jwt` checks a token's signature with each key's own algorithm, then its times against the clock, its issuer and its audience. Its certificate is read from DER. See `domains/lettings/libraries/council.jadpo`, `domains/lettings/actions/council_sign_in.jadpo` and `domains/lettings/actions/council_certificate.jadpo`.
11. **Settings in tests.** Every test reads each setting's `test:` value or its default, and gets throwaway keys of its own; a test's `config:` changes a setting for itself, and `clock.advance` moves its clock on. See `domains/lettings/actions/contractor_visit.test.jadpo`, `domains/lettings/actions/council_sign_in.test.jadpo` and [area 13, Configuration in tests](../../docs/language/13-testing.md#configuration-in-tests).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/13-lettings-office
target/release/jadpo test examples/13-lettings-office
target/release/jadpo fmt --check examples/13-lettings-office
cd examples/13-lettings-office && ../../target/release/jadpo config check
```

- `check` ends with `checked: <n> files` and no errors.
- `test` ends with `tested: 18 passed, 18 action tests`.
- `config check` lists each setting as `set`, `default` or `missing`
  with the variable it reads, never a value, and fails while a
  required one is missing. `jadpo config set notes_key` asks for a value
  at a prompt and keeps it in `.env.local`, which git ignores.

## Try next

- In `deposits.jadpo`, raise `cap_weeks` from 5 to 9. `jadpo check` reports `type.invalid_literal`: a constant is checked while compiling against its type, and `DepositWeeks` stops at 8.
  <!-- try {"file": "domains/lettings/libraries/deposits.jadpo", "replace": "constant cap_weeks: DepositWeeks = 5", "with": "constant cap_weeks: DepositWeeks = 9", "check": ["type.invalid_literal"]} -->
- In `deposits.jadpo`, raise `cap_weeks` from 5 to 6. The deposit's test fails with `test.assertion_failed`: every read of the constant changes with it.
  <!-- try {"file": "domains/lettings/libraries/deposits.jadpo", "replace": "constant cap_weeks: DepositWeeks = 5", "with": "constant cap_weeks: DepositWeeks = 6", "test": ["test.assertion_failed"], "failed": 1} -->
- In `config.jadpo`, delete `office_name`'s `test:` value. `jadpo check` reports `configuration.test_value_missing`: the receipts' tests read a setting that has no value in a test.
  <!-- try {"file": "config.jadpo", "replace": "    office_name: OfficeName {\n        test: \"Harbour Lettings\"\n    }", "with": "    office_name: OfficeName", "check": ["configuration.test_value_missing"]} -->
- In `acknowledge_payment.jadpo`, build the body from `config.payments_webhook_secret` instead of the payment. `jadpo check` reports `configuration.secret_flow`: a key goes only to a `crypto` call's `key:`; and `type.mismatch`, since a key is no value `json.write` can write.
  <!-- try {"file": "domains/lettings/actions/acknowledge_payment.jadpo", "replace": "WebhookBody(json.write(payment))", "with": "WebhookBody(json.write(config.payments_webhook_secret))", "check": ["configuration.secret_flow", "type.mismatch"]} -->
- In `contractor_visit.test.jadpo`, set `urgent_repair_within` to `45d` in the last test's `config:`. `jadpo check` reports the rule it breaks: a test's value is checked as a `default:` is, against `RepairWindow`'s `max: 30d`.
  <!-- try {"file": "domains/lettings/actions/contractor_visit.test.jadpo", "replace": "urgent_repair_within: 4h", "with": "urgent_repair_within: 45d", "check": ["configuration.default_invalid"]} -->
- Then read [14-studio-timetable](../14-studio-timetable/README.md), which works with time.
