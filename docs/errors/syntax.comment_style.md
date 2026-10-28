# `syntax.comment_style`

A comment starts with `//`, or `///` to document a declaration; `#` and `/* */` are other languages'.

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
  help: docs/language/01-project-layout-and-modules.html#syntax-comment-style

check failed: 1 error, 1 file
```
