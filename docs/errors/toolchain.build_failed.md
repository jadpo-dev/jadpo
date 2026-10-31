# `toolchain.build_failed`

The Rust `jadpo build` wrote for a deployable didn't compile, because the machine lacks something the build runs, such as a C compiler, or because of a mistake in Jadpo.

## Why Jadpo refuses it

`jadpo build` turns each deployable into Rust and has cargo compile it
with the toolchain this Jadpo pins. Your project has already passed
`jadpo check` by then, so a failure here is not in your code: either the
machine lacks something the build needs, or Jadpo wrote Rust that
doesn't compile, which is Jadpo's own mistake. The build stops, the
deployable's folder in `build/target/` is left as it was, and the reason
ends with the last lines cargo printed.

Jadpo won't install anything on your machine for you, and it won't
guess its way around a compiler error in Rust it wrote.

## What it looks like

A machine that can't run the build raises this error, so this output is
copied from a real run rather than checked by the docs. It is a build on
a Mac where no C compiler, `cc`, is on the `PATH`, which a native build
needs to link the program:

```sh
jadpo build
```

```text
Fetching the runtime's crates
error: `app` didn't compile
  reason: cargo couldn't build the Rust Jadpo wrote for it in `build/rust/app/`, which is Jadpo's own mistake or a toolchain it can't run: cargo couldn't build it:
          error: linker `cc` not found
            |
            = note: No such file or directory (os error 2)
          error: could not compile `proc-macro2` (build script) due to 1 previous error
          error: could not compile `quote` (build script) due to 1 previous error
          error: could not compile `libm` (build script) due to 1 previous error
          error: could not compile `libc` (build script) due to 1 previous error
  next: Run `jadpo build` again; if it fails the same way, report it with this output (human)
  rule: toolchain.build_failed
  help: https://jadpo.dev/docs/errors/toolchain.build_failed
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides what is installed on your machine, or whether to report the
failure to Jadpo's maintainers.

cargo's own lines follow `cargo couldn't build it:`, one under another
beneath the reason, and say what went wrong. Crates cargo can't
fetch before the build aren't this error but
[`toolchain.fetch_failed`](toolchain.fetch_failed.md).

## How to fix it

The question is what cargo's lines blame.

**If they name a tool the machine lacks**, such as the linker `cc`,
install a C compiler, then build again. On macOS:

```sh
xcode-select --install
jadpo build
```

On Debian or Ubuntu, `sudo apt install build-essential` installs one.
The build then links the program and writes it to `build/target/`.

**If they name a compiler error in the Rust Jadpo wrote**, run
`jadpo build` once more, and if it fails the same way, report it to
Jadpo's maintainers with the whole output:

```sh
jadpo build
```

The Rust is in `build/rust/<deployable>/`, so the report can point at
the line cargo refused.

## Related

- [What it compiles with](../language/14-tooling.md#what-it-compiles-with)
- [A native deployable](../language/14-tooling.md#a-native-deployable)
- [`jadpo build`](../language/14-tooling.md#jadpo-build)
