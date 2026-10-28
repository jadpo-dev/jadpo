# `syntax.export_keyword`

Every declaration is visible across its domain without `export` or `pub`; `public` opens one to other domains.

```jadpo check project=e-habit-export expect=syntax.export_keyword
type Count = Int {
    min: 0
}

export function same(count: Count) -> Count {
    return count
}
```

```text output
error: Jadpo has no `export`
 --> main.jadpo:5:1
  |
5 | export function same(count: Count) -> Count {
  | ^^^^^^ remove `export`
  reason: `export` makes a name visible to other files in JavaScript and TypeScript, and `pub` outside its module in Rust; every Jadpo declaration is visible across its domain, and `public` opens it to other domains.
  next: Remove `export` (agent)
  rule: syntax.export_keyword
  help: docs/language/01-project-layout-and-modules.html#syntax-export-keyword

check failed: 1 error, 1 file
```
