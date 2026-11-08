# `type.reserved_variant_name`

An enum declares a variant called `all`, `true`, `false` or `none`, which already mean something else.

## Why Jadpo refuses it

`Newsletter.all` lists every variant of `Newsletter`, for an enum whose
variants carry no data, so it can't also be a variant. A standard closed
list whose code is `all` writes it with a trailing `_`, as Albania's lek
is `Currency.all_`.

## What it looks like

main.jadpo:

```jadpo check project=e-reserved-variant-name expect=type.reserved_variant_name
type Newsletter = Enum {
    subscribed
    all
}
```

```text output
error: `all` can't be a variant of `Newsletter`
 --> main.jadpo:3:5
  |
3 |     all
  |     ^^^ reserved
  reason: `Newsletter.all` lists every variant of `Newsletter`, so no variant takes the name `all`.
  next: Rename the variant (agent)
  rule: type.reserved_variant_name
  help: https://jadpo.dev/docs/errors/type.reserved_variant_name

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`true`, `false` and `none` are values, so no variant takes those names
either: beside a `Compression?`, `Compression.none` would read as the
`none` it isn't. Any other reserved word may name a variant.

main.jadpo:

```jadpo check project=e-reserved-variant-value expect=type.reserved_variant_name
type Compression = Enum {
    gzip
    none
}
```

```text output
error: `none` can't be a variant of `Compression`
 --> main.jadpo:3:5
  |
3 |     none
  |     ^^^^ reserved
  reason: `none` is a value in its own right, and `Compression.none` beside it would read as that value, which it isn't.
  next: Rename the variant (agent)
  rule: type.reserved_variant_name
  help: https://jadpo.dev/docs/errors/type.reserved_variant_name

check failed: 1 error, 1 file
```

## How to fix it

**If the variant is called `all`**, name it for what it means:

```jadpo fragment
type Newsletter = Enum {
    subscribed
    everything
}
```

```jadpo check project=e-reserved-variant-name-fix hidden
type Newsletter = Enum {
    subscribed
    everything
}
```

`Newsletter.all` now lists both variants.

**If the variant is called `true`, `false` or `none`**, name it for what
that case is:

```jadpo fragment
type Compression = Enum {
    gzip
    uncompressed
}
```

```jadpo check project=e-reserved-variant-value-fix hidden
type Compression = Enum {
    gzip
    uncompressed
}
```

`Compression.uncompressed` is now a value of the enum, and `none` still
means no value.

## Related

- [Enums](../language/02-values-and-types.md#enums)
- [Names from reserved words](../language/02-values-and-types.md#names-from-reserved-words)
- [Closed lists are enums](../language/19-standard-library.md#closed-lists-are-enums)
