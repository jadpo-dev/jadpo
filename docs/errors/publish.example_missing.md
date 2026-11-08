# `publish.example_missing`

A function of a package's public library has no example in its description, or can fail in a way no example or test shows.

## Why Jadpo refuses it

An example in a function's `///` description is what `jadpo package
show` and the registry's page show of how it is called, and it runs as a
test, so it always says what the code does. A function without one
leaves whoever installs the package to work out the call from its
signature. Each failure a function can raise is something an app has to
handle, so an example or one of the package's tests shows when it
happens, by asserting it with `fails`. `jadpo package check` fails its
`examples` row until every function has an example and every failure is
shown.

## What it looks like

A package started with `jadpo package new`, its starter function's
example taken out of its description and its failure asserted in a test
instead:

```sh run project=e-example-missing expect=publish.example_missing
jadpo package new Postcodes --library > /dev/null
cd postcodes
cat > libraries/starter.jadpo <<'DOC'
/// A first function, showing the shape of one. Replace it with your own.
public library starter {
    /// Takes the spaces off both ends of what someone typed.
    function tidied(entered: EnteredText) fails NothingEntered -> TidyText {
        return attempt TidyText(text.trim(entered)) {
            invalid: NothingEntered
        }
    }
}
DOC
cat > libraries/starter.test.jadpo <<'DOC'
test "nothing but spaces is nothing entered" {
    assert starter.tidied("   ") fails NothingEntered
}
DOC
jadpo package check
```

```text output
error: Function `starter.tidied` has no example
 --> libraries/starter.jadpo:4:14
  |
4 |     function tidied(entered: EnteredText) fails NothingEntered -> TidyText {
  |              ^^^^^^ no example in its description
  reason: Every function of a published library shows how it is called with an example in its description, a ```jadpo block run as a test, so what the registry's page and `jadpo package show` say of it always runs.
  next: Add a ```jadpo block to `starter.tidied`'s `///` description, holding statements as a test does: `assert starter.tidied(...) == ...` (agent)
  rule: publish.example_missing
  help: https://jadpo.dev/docs/errors/publish.example_missing

Facts
  kind: library, pure
  storable: EnteredText, TidyText
  failures: starter.tidied: NothingEntered

Publishing bar
  compiles      passed                        no errors
  described     passed                        every public declaration
  examples      failed                        1 function has no example
  tests         passed                        1 test
  recipes       passed                        1 recipe compiles in sample/
  version       passed                        1.0.0, the first release
  changelog     passed                        1.0.0 says why
  publisher     left out                      publishing needs it
  licence       passed                        proprietary, registry refuses it

package check failed: 1 failed, 7 passed
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Without the test, the failure would be a second error of its own. A
function whose example doesn't show a failure it can raise gets that
error alone. Here the example keeps its first two lines and nothing else
asserts `NothingEntered`:

```sh run project=e-example-missing-failure expect=publish.example_missing
jadpo package new Postcodes --library > /dev/null
cd postcodes
cat > libraries/starter.jadpo <<'DOC'
/// A first function, showing the shape of one. Replace it with your own.
public library starter {
    /// Takes the spaces off both ends of what someone typed.
    ///
    /// ```jadpo
    /// var tidy: TidyText = attempt starter.tidied("  Hello ")
    /// assert tidy == "Hello"
    /// ```
    function tidied(entered: EnteredText) fails NothingEntered -> TidyText {
        return attempt TidyText(text.trim(entered)) {
            invalid: NothingEntered
        }
    }
}
DOC
jadpo package check
```

```text output
error: `starter.tidied` can fail with `NothingEntered`, which no example or test shows
 --> libraries/starter.jadpo:9:14
  |
9 |     function tidied(entered: EnteredText) fails NothingEntered -> TidyText {
  |              ^^^^^^ fails NothingEntered
  reason: A published function shows each failure it can raise, so an app knows when it happens: an example or a test asserts it with `fails`.
  next: Add `assert starter.tidied(...) fails NothingEntered` to its example or to a test beside it (agent)
  rule: publish.example_missing
  help: https://jadpo.dev/docs/errors/publish.example_missing

Facts
  kind: library, pure
  storable: EnteredText, TidyText
  failures: starter.tidied: NothingEntered

Publishing bar
  compiles      passed                        no errors
  described     passed                        every public declaration
  examples      failed                        1 failure not shown
  tests         passed                        1 test
  recipes       passed                        1 recipe compiles in sample/
  version       passed                        1.0.0, the first release
  changelog     passed                        1.0.0 says why
  publisher     left out                      publishing needs it
  licence       passed                        proprietary, registry refuses it

package check failed: 1 failed, 7 passed
```

## How to fix it

**If the function has no example**, add a ```` ```jadpo ```` block to
its description, holding statements as a test's body does: a call, and
an `assert` of what it gives and of each failure it can raise:

```jadpo fragment
    /// Takes the spaces off both ends of what someone typed.
    ///
    /// ```jadpo
    /// var tidy: TidyText = attempt starter.tidied("  Hello ")
    /// assert tidy == "Hello"
    /// assert starter.tidied("   ") fails NothingEntered
    /// ```
    function tidied(entered: EnteredText) fails NothingEntered -> TidyText {
```

```jadpo check project=e-example-missing-fixed file=shared/providers/postcodes/v1/libraries/starter.jadpo hidden
/// A first function, showing the shape of one. Replace it with your own.
public library starter {
    /// Takes the spaces off both ends of what someone typed.
    ///
    /// ```jadpo
    /// var tidy: TidyText = attempt starter.tidied("  Hello ")
    /// assert tidy == "Hello"
    /// assert starter.tidied("   ") fails NothingEntered
    /// ```
    function tidied(entered: EnteredText) fails NothingEntered -> TidyText {
        return attempt TidyText(text.trim(entered)) {
            invalid: NothingEntered
        }
    }
}
```

```jadpo check project=e-example-missing-fixed file=shared/providers/postcodes/v1/types/entered_text.jadpo hidden
/// Text as someone typed it.
public type EnteredText = Text {
    max_length: 200
}
```

```jadpo check project=e-example-missing-fixed file=shared/providers/postcodes/v1/types/tidy_text.jadpo hidden
/// Text with no spaces around it, and something in it.
public type TidyText = Text {
    min_length: 1
    max_length: 200
}
```

```jadpo check project=e-example-missing-fixed file=shared/providers/postcodes/v1/failures/nothing_entered.jadpo hidden
/// Nothing was left once the text was tidied.
public type NothingEntered = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=e-example-missing-fixed file=shared/providers/postcodes/v1/package.jadpo hidden
/// UK postcodes, read and tidied.
package Postcodes.v1 {
    licence: Licence.mit
    jadpo: "X.Y"
}
```

```jadpo check project=e-example-missing-fixed file=app.jadpo hidden
application {}
```

The example runs as a test of the package and is counted in the
`examples` row, and `jadpo package show` gives it with the function.

**If only the failure isn't shown**, and the example is better kept to
the usual call, assert the failure in a test beside the library instead,
in `libraries/starter.test.jadpo`:

```jadpo fragment
test "nothing but spaces is nothing entered" {
    assert starter.tidied("   ") fails NothingEntered
}
```

```jadpo check project=e-example-missing-test file=shared/providers/postcodes/v1/libraries/starter.test.jadpo hidden
test "nothing but spaces is nothing entered" {
    assert starter.tidied("   ") fails NothingEntered
}
```

```jadpo check project=e-example-missing-test file=shared/providers/postcodes/v1/libraries/starter.jadpo hidden
/// A first function, showing the shape of one. Replace it with your own.
public library starter {
    /// Takes the spaces off both ends of what someone typed.
    ///
    /// ```jadpo
    /// var tidy: TidyText = attempt starter.tidied("  Hello ")
    /// assert tidy == "Hello"
    /// ```
    function tidied(entered: EnteredText) fails NothingEntered -> TidyText {
        return attempt TidyText(text.trim(entered)) {
            invalid: NothingEntered
        }
    }
}
```

```jadpo check project=e-example-missing-test file=shared/providers/postcodes/v1/types/entered_text.jadpo hidden
/// Text as someone typed it.
public type EnteredText = Text {
    max_length: 200
}
```

```jadpo check project=e-example-missing-test file=shared/providers/postcodes/v1/types/tidy_text.jadpo hidden
/// Text with no spaces around it, and something in it.
public type TidyText = Text {
    min_length: 1
    max_length: 200
}
```

```jadpo check project=e-example-missing-test file=shared/providers/postcodes/v1/failures/nothing_entered.jadpo hidden
/// Nothing was left once the text was tidied.
public type NothingEntered = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=e-example-missing-test file=shared/providers/postcodes/v1/package.jadpo hidden
/// UK postcodes, read and tidied.
package Postcodes.v1 {
    licence: Licence.mit
    jadpo: "X.Y"
}
```

```jadpo check project=e-example-missing-test file=app.jadpo hidden
application {}
```

The test counts as showing the failure, and runs in the `tests` row with
the package's other tests.

## Related

- [What publishing requires](../language/11-integrations.md#what-publishing-requires)
- [How the surface and the publishing bar work](../language/11-integrations.md#how-the-surface-and-the-publishing-bar-work)
- [`jadpo package check`](../language/14-tooling.md#jadpo-package-check)
