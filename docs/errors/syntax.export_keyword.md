# `syntax.export_keyword`

A declaration is marked `export` or `pub`.

## Why Jadpo refuses it

`export` opens a name to other files in JavaScript and TypeScript, and `pub`
opens it outside its module in Rust. Every Jadpo declaration is already
visible across its domain, and `public` is the one word that opens it to
other domains, so what can see a declaration is one fact, written in one
place.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/syntax.export_keyword

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If only its own domain uses it**, remove `export`:

```jadpo fragment
function same(count: Count) -> Count {
    return count
}
```

```jadpo check project=e-habit-export-fix-1 hidden
type Count = Int {
    min: 0
}

function same(count: Count) -> Count {
    return count
}
```

`same` is now visible across its own domain and no further.

**If another domain calls it**, make it a public action, since a function
stays with the code that calls it:

```jadpo fragment
public action same(count: Count) -> Count {
    return count
}
```

```jadpo check project=e-habit-export-fix-2 hidden
type Count = Int {
    min: 0
}

public action same(count: Count) -> Count {
    return count
}
```

Other domains can now call `same`.

## Related

- [What can be public](../language/01-project-layout-and-modules.md#what-can-be-public)
- [Ownership and calls replace modules](../language/01-project-layout-and-modules.md#ownership-and-calls-replace-modules)
