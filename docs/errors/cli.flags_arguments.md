# `cli.flags_arguments`

A `jadpo flags` command names an action it doesn't have, a flag the project doesn't declare, or a value, user or share the flag can't take.

## Why Jadpo refuses it

`jadpo flags` changes who gets which value of a flag while the app runs,
and every change reaches every server within ten seconds. A flag, value
or share it doesn't know would change the wrong thing, or nothing, so it
is refused before the database is touched.

## What it looks like

A project with one flag, an enum of two layouts:

```jadpo check project=e-flags-arguments file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-flags-arguments file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-flags-arguments file=domains/ordering/types/checkout_layout.jadpo hidden
/// How the checkout's form is laid out.
type CheckoutLayout = Enum {
    classic
    single_column
}
```

```jadpo check project=e-flags-arguments file=flags.jadpo migrate=initial
use ordering/types { CheckoutLayout }

flags {
    /// Which layout the checkout uses while we compare two.
    checkout_layout: CheckoutLayout {
        default: CheckoutLayout.classic
        owner: "payments"
        expires: "2099-12-15"
    }
}
```

Given a value the flag doesn't have:

```sh run project=e-flags-arguments expect=cli.flags_arguments
jadpo flags set checkout_layout single_colum
```

```text output
error: `single_colum` isn't a value of `checkout_layout`
  reason: `checkout_layout` is `classic` or `single_column`. Did you mean `single_column`?
  next: Use `single_column` instead of `single_colum` (agent)
  rule: cli.flags_arguments
  help: https://jadpo.dev/docs/errors/cli.flags_arguments
```

A split whose shares add up to more than everyone:

```sh run project=e-flags-arguments expect=cli.flags_arguments
jadpo flags set checkout_layout --split classic=60,single_column=60
```

```text output
error: `--split` gives 120% of users
  reason: Each user is in one share at most, so the shares add up to 100% or less; the users in none get everyone's value.
  next: Make the percentages add up to 100 or less (agent)
  rule: cli.flags_arguments
  help: https://jadpo.dev/docs/errors/cli.flags_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the value is misspelt**, write it as the flag's type declares it:

```sh
jadpo flags set checkout_layout single_column
```

Everyone without a named value or a share gets `single_column`.

**If the shares add up to more than 100**, give each user one share at
most:

```sh
jadpo flags set checkout_layout --split classic=50,single_column=50
```

Half the users get each layout.

## Related

- [`jadpo flags`](../language/14-tooling.md#jadpo-flags)
- [Flags: changed, listed and removed](../language/16-built-in-services.md#flags-changed-listed-and-removed)
