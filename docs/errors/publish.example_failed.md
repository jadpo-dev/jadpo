# `publish.example_failed`

An example in a package's `///` description doesn't compile, or doesn't hold when it runs as a test.

## Why Jadpo refuses it

Each ```` ```jadpo ```` block in a public declaration's description is
what `jadpo package show` and the registry's page show of how it is
called, and an agent copies it. `jadpo package check` runs each one as a
test of the package, so what a description shows is always what the code
does. An example that fails would teach whoever reads it something
untrue, so the `examples` row fails until it holds. Jadpo can't tell
whether the example or the code is the one that's wrong, so it shows the
example's line and leaves that to you.

## What it looks like

A package started with `jadpo package new`, its starter function's
example expecting the text in lower case:

```sh run project=e-example-failed expect=publish.example_failed
jadpo package new Postcodes --library > /dev/null
cd postcodes
cat > libraries/starter.jadpo <<'DOC'
/// A first function, showing the shape of one. Replace it with your own.
public library starter {
    /// Takes the spaces off both ends of what someone typed.
    ///
    /// ```jadpo
    /// var tidy: TidyText = attempt starter.tidied("  Hello ")
    /// assert tidy == "hello"
    /// assert starter.tidied("   ") fails NothingEntered
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
error: Example of `starter.tidied` failed
 --> libraries/starter.jadpo:7:9
  |
7 |     /// assert tidy == "hello"
  |         ^^^^^^^^^^^^^^^^^^^^^^ this doesn't hold
  reason: Run as a test of the package, it expected "hello" and got "Hello". An example runs as a test, so a description always shows what the code does.
  next: Correct the example, or the code if the example says what it should do (agent)
  rule: publish.example_failed
  help: https://jadpo.dev/docs/errors/publish.example_failed

Facts
  kind: library, pure
  storable: EnteredText, TidyText
  failures: starter.tidied: NothingEntered

Publishing bar
  compiles      passed                        no errors
  described     passed                        every public declaration
  examples      failed                        1 of 1 example failed
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

An example that doesn't compile is shown at its line the same way, with
what the compiler said. Here it compares the tidied text with a number:

```sh run project=e-example-failed-compile expect=publish.example_failed
jadpo package new Postcodes --library > /dev/null
cd postcodes
cat > libraries/starter.jadpo <<'DOC'
/// A first function, showing the shape of one. Replace it with your own.
public library starter {
    /// Takes the spaces off both ends of what someone typed.
    ///
    /// ```jadpo
    /// var tidy: TidyText = attempt starter.tidied("  Hello ")
    /// assert tidy == 5
    /// assert starter.tidied("   ") fails NothingEntered
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
error: Example of `starter.tidied` doesn't compile
 --> libraries/starter.jadpo:7:9
  |
7 |     /// assert tidy == 5
  |         ^^^^^^^^^^^^^^^^ doesn't compile
  reason: Run as a test of the package, it gives: `Postcodes.TidyText` and a whole number can't be compared. An example runs as a test, so a description always shows what the code does.
  next: Correct the example, writing statements as a test's body does (agent)
  rule: publish.example_failed
  help: https://jadpo.dev/docs/errors/publish.example_failed

Facts
  kind: library, pure
  storable: EnteredText, TidyText
  failures: starter.tidied: NothingEntered

Publishing bar
  compiles      passed                        no errors
  described     passed                        every public declaration
  examples      failed                        1 of 1 example failed
  tests         passed                        1 test
  recipes       passed                        1 recipe compiles in sample/
  version       passed                        1.0.0, the first release
  changelog     passed                        1.0.0 says why
  publisher     left out                      publishing needs it
  licence       passed                        proprietary, registry refuses it

package check failed: 1 failed, 7 passed
```

The example is left out of the run, so the package's other examples
still run and are counted.

## How to fix it

The question is which one says what the function should do: the example
or the code.

**If the example is wrong**, correct it to what the code gives:

```jadpo fragment
public library starter {
    ...
    /// var tidy: TidyText = attempt starter.tidied("  Hello ")
    /// assert tidy == "Hello"
    ...
}
```

```jadpo check project=e-example-failed-fixed file=shared/providers/postcodes/v1/libraries/starter.jadpo hidden
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

```jadpo check project=e-example-failed-fixed file=shared/providers/postcodes/v1/types/entered_text.jadpo hidden
/// Text as someone typed it.
public type EnteredText = Text {
    max_length: 200
}
```

```jadpo check project=e-example-failed-fixed file=shared/providers/postcodes/v1/types/tidy_text.jadpo hidden
/// Text with no spaces around it, and something in it.
public type TidyText = Text {
    min_length: 1
    max_length: 200
}
```

```jadpo check project=e-example-failed-fixed file=shared/providers/postcodes/v1/failures/nothing_entered.jadpo hidden
/// Nothing was left once the text was tidied.
public type NothingEntered = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=e-example-failed-fixed file=shared/providers/postcodes/v1/package.jadpo hidden
/// UK postcodes, read and tidied.
package Postcodes.v1 {
    licence: Licence.mit
    jadpo: "X.Y"
}
```

```jadpo check project=e-example-failed-fixed file=app.jadpo hidden
application {}
```

The `examples` row passes once every example holds, and the description
shows what `starter.tidied` really returns.

**If the code is wrong**, change the code to do what the example says,
and leave the example as it is:

```jadpo fragment
    function tidied(entered: EnteredText) fails NothingEntered -> TidyText {
        return attempt TidyText(text.lower(text.trim(entered))) {
            ...
        }
    }
```

```jadpo check project=e-example-failed-code file=shared/providers/postcodes/v1/libraries/starter.jadpo hidden
/// A first function, showing the shape of one. Replace it with your own.
public library starter {
    /// Takes the spaces off both ends of what someone typed, and gives
    /// it in lower case.
    ///
    /// ```jadpo
    /// var tidy: TidyText = attempt starter.tidied("  Hello ")
    /// assert tidy == "hello"
    /// assert starter.tidied("   ") fails NothingEntered
    /// ```
    function tidied(entered: EnteredText) fails NothingEntered -> TidyText {
        return attempt TidyText(text.lower(text.trim(entered))) {
            invalid: NothingEntered
        }
    }
}
```

```jadpo check project=e-example-failed-code file=shared/providers/postcodes/v1/types/entered_text.jadpo hidden
/// Text as someone typed it.
public type EnteredText = Text {
    max_length: 200
}
```

```jadpo check project=e-example-failed-code file=shared/providers/postcodes/v1/types/tidy_text.jadpo hidden
/// Text with no spaces around it, and something in it.
public type TidyText = Text {
    min_length: 1
    max_length: 200
}
```

```jadpo check project=e-example-failed-code file=shared/providers/postcodes/v1/failures/nothing_entered.jadpo hidden
/// Nothing was left once the text was tidied.
public type NothingEntered = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=e-example-failed-code file=shared/providers/postcodes/v1/package.jadpo hidden
/// UK postcodes, read and tidied.
package Postcodes.v1 {
    licence: Licence.mit
    jadpo: "X.Y"
}
```

```jadpo check project=e-example-failed-code file=app.jadpo hidden
application {}
```

`starter.tidied` now gives what someone typed in lower case, so the
example holds and the `examples` row passes. Say so in the function's
description too, since that is what apps read.

## Related

- [What publishing requires](../language/11-integrations.md#what-publishing-requires)
- [How the surface and the publishing bar work](../language/11-integrations.md#how-the-surface-and-the-publishing-bar-work)
- [`jadpo package check`](../language/14-tooling.md#jadpo-package-check)
