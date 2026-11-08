# `syntax.comment_style`

A comment is written with `#` or `/* */` instead of `//`.

## Why Jadpo refuses it

`#` starts a comment in Python, Ruby, shell scripts and YAML, and `/* */`
encloses one in the C family and SQL. Jadpo has one form, `//` to the end of
the line, and `///` for a comment that documents a declaration, so what a
comment is for can be read from how it starts.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-comment expect=syntax.comment_style
# Counts start at zero.
type Count = Int {
    min: 0
}
```

```text output
error: Jadpo writes comments with `//`, not `#`
 --> main.jadpo:1:1
  |
1 | # Counts start at zero.
  | ^ write `//`
  reason: `#` starts a comment in Python, Ruby, shell scripts and YAML; a Jadpo comment starts with `//`, or `///` to document a declaration.
  next: Use `//` instead of `#` (agent)
  rule: syntax.comment_style
  help: https://jadpo.dev/docs/errors/syntax.comment_style

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A `/* */` comment is this error too, and its `next:` tells you to put each
line of the comment after `//`:

main.jadpo:

```jadpo check project=e-habit-comment-2 expect=syntax.comment_style
/* Counts start at zero. */
type Count = Int {
    min: 0
}
```

```text output
error: Jadpo writes comments with `//`, not `/* */`
 --> main.jadpo:1:1
  |
1 | /* Counts start at zero. */
  | ^^ write `//`
  reason: `/* */` encloses a comment in JavaScript, Rust, Go, Java, C#, Kotlin, Swift and SQL, and `/** */` documents a declaration; a Jadpo comment is `//` to the end of the line, and `///` documents.
  next: Write each line of the comment after `//` (agent)
  rule: syntax.comment_style
  help: https://jadpo.dev/docs/errors/syntax.comment_style

check failed: 1 error, 1 file
```

## How to fix it

**If it is a note for whoever reads the code**, start it with `//`:

```jadpo check project=e-habit-comment-fix-1
// Counts start at zero.
type Count = Int {
    min: 0
}
```

The line is now a comment, with no meaning to the compiler, and a `/* */`
comment gets one `//` for each of its lines.

**If it documents the declaration below it**, start it with `///`:

```jadpo check project=e-habit-comment-fix-2
/// Counts start at zero.
type Count = Int {
    min: 0
}
```

The line now documents `Count`, so editors and the generated documentation
show it.

## Related

- [Comments](../language/01-project-layout-and-modules.md#comments)
