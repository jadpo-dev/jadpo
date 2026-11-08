# `semantic.name_case`

A name has the wrong case for what it names.

## Why Jadpo refuses it

Case says what a name is, at a glance: types, entities and route groups
are written in UpperCamelCase, and functions, actions, queries, fields,
variables and the rest in lower_snake_case. A name written the other way
would make the code read as something it isn't.

## What it looks like

checkout.jadpo:


```jadpo check project=e-case file=checkout.jadpo expect=semantic.name_case
type Stock = Int {
    min: 0
}

function orderTotal(subtotal: Stock) -> Stock {
    return subtotal
}
```

```text output
error: Function `orderTotal` isn't written in lower_snake_case
 --> checkout.jadpo:5:10
  |
5 | function orderTotal(subtotal: Stock) -> Stock {
  |          ^^^^^^^^^^ should be `order_total`
  reason: Types, entities and route groups are written in UpperCamelCase; functions, actions, queries, fields, variables and the rest in lower_snake_case.
  next: Rename `orderTotal` to `order_total`, here and where it's used (agent)
  rule: semantic.name_case
  help: https://jadpo.dev/docs/errors/semantic.name_case

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A type that isn't in UpperCamelCase is refused the same way:

```jadpo check project=e-case-type expect=semantic.name_case
type order_total = Int {
    min: 0
}
```

```text output
error: Type `order_total` isn't written in UpperCamelCase
 --> main.jadpo:1:6
  |
1 | type order_total = Int {
  |      ^^^^^^^^^^^ should be `OrderTotal`
  reason: Types, entities and route groups are written in UpperCamelCase; functions, actions, queries, fields, variables and the rest in lower_snake_case.
  next: Rename `order_total` to `OrderTotal`, here and where it's used (agent)
  rule: semantic.name_case
  help: https://jadpo.dev/docs/errors/semantic.name_case

check failed: 1 error, 1 file
```

A domain's name is its folder's name, so a folder in the wrong case has to
be renamed:

```jadpo check project=e-case-domain file=app.jadpo hidden
application {}
```

```jadpo check project=e-case-domain file=domains/OrderLines/actions/tidy.jadpo expect=semantic.name_case
action tidy() -> Unit {}
```

```text output
error: Domains are written in lower_snake_case
 --> domains/OrderLines/actions/tidy.jadpo
  reason: A domain's folder name is its name, used in calls such as `order_lines.action(...)`.
  next: Rename the folder `domains/OrderLines/` to `domains/order_lines/` (agent)
  rule: semantic.name_case
  help: https://jadpo.dev/docs/errors/semantic.name_case

check failed: 1 error, 2 files
```

## How to fix it

**If it's a function, field or other lower-case name**, write it in
lower_snake_case, here and where it's used:

```jadpo fragment
function order_total(subtotal: Stock) -> Stock {
    return subtotal
}
```

```jadpo check project=e-case-fix file=checkout.jadpo hidden
type Stock = Int {
    min: 0
}

function order_total(subtotal: Stock) -> Stock {
    return subtotal
}
```

The name now reads as what it is.

## Related

- [Naming](../language/01-project-layout-and-modules.md#naming)
