# `semantic.duplicate_declaration`

```jadpo check project=e-duplicate file=a.jadpo hidden
type Count = Int {
    min: 0
}
```

```jadpo check project=e-duplicate file=b.jadpo expect=semantic.duplicate_declaration
type Count = Int {
    min: 1
}
```

```text output
error: `Count` is declared twice
 --> b.jadpo:1:6
  |
1 | type Count = Int {
  |      ^^^^^ declared again here
  reason: `Count` is already a type in `a.jadpo`, declared differently, so these are two things with one name.
  next: Rename one of them, here and where it's used (agent)
  in use:
    Count  a.jadpo
  rule: semantic.duplicate_declaration
  help: https://jadpo.dev/docs/errors/semantic.duplicate_declaration

check failed: 1 error, 2 files
```

The error names both declarations and their files, and says which of two
cases it is. If the two
are written the same way, ignoring layout, comments and `public`, it is a
copy: remove one and use the other, or move it to `shared/` if two domains
need it. If they differ, they are two things with one name: rename one. It
never invents a name. It lists the names in use that contain the same
words, with their files, so the agent renaming one sees what is taken and
what the convention is. A private function or type needs a name unique
only within its domain, so here `shipping`'s own `Address` is no clash,
while a second one in `billing` is:

```jadpo check project=e-duplicate-domain file=app.jadpo hidden
application {}
```

```jadpo check project=e-duplicate-domain file=domains/billing/actions/send_invoice.jadpo hidden
type Address = Object {
    street: Text
}

action send_invoice(address: Address) -> Unit {}
```

```jadpo check project=e-duplicate-domain file=domains/shipping/actions/ship_parcel.jadpo hidden
type Address = Object {
    postcode: Text
}

action ship_parcel(address: Address) -> Unit {}
```

```jadpo check project=e-duplicate-domain file=domains/shipping/actions/print_label.jadpo hidden
type ShippingAddress = Object {
    line_one: Text
}

action print_label(address: ShippingAddress) -> Unit {}
```

```jadpo check project=e-duplicate-domain file=domains/billing/actions/send_reminder.jadpo expect=semantic.duplicate_declaration
type Address = Object {
    street: Text
    town: Text
}

action send_reminder(address: Address) -> Unit {}
```

```text output
error: `Address` is declared twice in `billing`
 --> domains/billing/actions/send_reminder.jadpo:1:6
  |
1 | type Address = Object {
  |      ^^^^^^^ declared again here
  reason: `Address` is already a type in `domains/billing/actions/send_invoice.jadpo`, declared differently, so these are two things with one name.
  next: Rename one of them, here and where it's used (agent)
  in use:
    Address          domains/billing/actions/send_invoice.jadpo
    Address          domains/shipping/actions/ship_parcel.jadpo
    ShippingAddress  domains/shipping/actions/print_label.jadpo
  rule: semantic.duplicate_declaration
  help: https://jadpo.dev/docs/errors/semantic.duplicate_declaration

check failed: 1 error, 5 files
```

The next step says "Rename one of them" without naming `jadpo rename`,
which is not built yet (area 14, "Renaming"). A name contains another's words when
its words, split as the file name splits them, include them in order
(`ShippingAddress` and `format_address` contain `Address`'s); the list
gives each file's path from the project root, as every other error does,
and shows at most twelve names, the twelfth line counting the rest. A name
declared twice in one scope is this error alone: no import or promotion is
asked for either copy.
