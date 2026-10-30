# `syntax.expected_name`

```jadpo check project=e-expected-name expect=syntax.expected_name
type Pair = Object {
    : Int
}
```

```text output
error: This member has no name
 --> main.jadpo:2:5
  |
2 |     : Int
  |     ^ expected a name before `:`
  reason: Every member inside braces is `name: value`.
  next: Add the member's name before `:` (agent)
  rule: syntax.expected_name
  help: https://jadpo.dev/docs/errors/syntax.expected_name

check failed: 1 error, 1 file
```

A domain is named by its folder, so a folder named with a reserved word is
refused there, once. Where the domain is used, `library.borrow(...)`, the
word is read as its name and nothing more is reported:

```jadpo check project=e-expected-name-domain file=app.jadpo hidden
application {}
```

```jadpo check project=e-expected-name-domain file=domains/desk/actions/lend_book.jadpo
use library

action lend_book() -> Unit {
    library.borrow()
}
```

```jadpo check project=e-expected-name-domain file=domains/library/actions/borrow.jadpo expect=syntax.expected_name
public action borrow() -> Unit {}
```

```text output
error: `library` is a reserved word and can't name a domain
 --> domains/library/actions/borrow.jadpo
  reason: A domain's folder name is its name, written before each call into it, and a reserved word can't be a name: `library.` would read as the word, not the domain.
  next: Rename the folder `domains/library/` after the business area it covers, and write the new name wherever `library.` names the domain (human)
  rule: syntax.expected_name
  help: https://jadpo.dev/docs/errors/syntax.expected_name

check failed: 1 error, 3 files
```

A block whose names are wrong reports the first: a later line is most
often the same mistake again, and is reported once the first is fixed.
