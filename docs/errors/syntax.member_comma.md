# `syntax.member_comma`

Each member is on its own line, with no commas between them.

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
