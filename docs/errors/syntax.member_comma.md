# `syntax.member_comma`

Members inside braces are separated by commas.

## Why Jadpo refuses it

Commas separate fields in JavaScript, TypeScript, Rust, Go, Swift and JSON.
Each Jadpo member is on its own line, and the formatter puts it there, so a
comma would be a second way of writing the same thing. A call's arguments are
different: they keep their commas, inside brackets.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-member-comma expect=syntax.member_comma
type Stock = Int {
    min: 0
}

type PriceRange = Object {
    low: Stock,
    high: Stock
}
```

```text output
error: Jadpo has no `,` between members
 --> main.jadpo:6:15
  |
6 |     low: Stock,
  |               ^ remove `,`
  reason: Commas separate fields in JavaScript, TypeScript, Rust, Go, Swift and JSON; each Jadpo member is on its own line, and the formatter puts it there.
  next: Remove `,` (agent)
  rule: syntax.member_comma
  help: https://jadpo.dev/docs/errors/syntax.member_comma

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you put a comma after a member**, remove it:

```jadpo fragment
type PriceRange = Object {
    low: Stock
    high: Stock
}
```

```jadpo check project=e-habit-member-comma-fix hidden
type Stock = Int {
    min: 0
}

type PriceRange = Object {
    low: Stock
    high: Stock
}
```

`PriceRange` now has its two fields, one on each line.

## Related

- [Objects](../language/02-values-and-types.md#objects)
- [Habits from other languages](../language/03-logic.md#habits-from-other-languages)
