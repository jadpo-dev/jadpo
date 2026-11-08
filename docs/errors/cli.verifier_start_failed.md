# `cli.verifier_start_failed`

The verifier won't start, for want of its credential or a sandbox to run packages in, or because it can't listen.

## Why Jadpo refuses it

The verifier runs code from anyone: a package's tests and examples, and its
last release's. It runs them only in a sandbox with no network, no files of
the machine it is on and limits on what they can use, and it answers only
the registry, which proves itself with a credential. Without either it
doesn't serve at all, since serving with less would be a way in that looks
like the real thing.

So it checks both before it listens: the credential is there and long
enough, and a program in the sandbox really has no network. It serves only
on Linux, with [bubblewrap](https://github.com/containers/bubblewrap); its
container image has everything it needs.

## What it looks like

Started with no credential:

```jadpo check project=e-verifier-start file=app.jadpo hidden
application {}
```

```sh run project=e-verifier-start expect=cli.verifier_start_failed
jadpo package verify --serve
```

```text output
error: `jadpo package verify --serve` has no credential
  reason: It answers only the registry, which proves itself with a credential in `JADPO_VERIFIER_TOKEN`, of at least 32 characters.
  next: Set JADPO_VERIFIER_TOKEN to the secret the registry holds (human)
  rule: cli.verifier_start_failed
  help: https://jadpo.dev/docs/errors/cli.verifier_start_failed
```

`(human)` at the end of the `next:` line tells your coding agent to stop and
ask you which fix you want, because the choice is yours to make: it decides
what the verifier trusts.

## How to fix it

**If the credential is missing**, set it to the secret the registry holds, at
least 32 characters, such as one `openssl rand -hex 32` prints. On Cloudflare,
where the registry runs the verifier, it is the Worker's secret, and Wrangler
asks for its value rather than taking it on the command line:

```sh
npx wrangler secret put JADPO_VERIFIER_TOKEN
```

The verifier then starts and answers only a caller that sends the same
secret, so give the registry the same value.

**If it has no sandbox**, run it from its container image, which brings
bubblewrap, `prlimit` and `git`. In a plain Docker container the sandbox
needs its namespaces allowed:

```sh
docker run --security-opt seccomp=unconfined --security-opt apparmor=unconfined --security-opt systempaths=unconfined -e JADPO_VERIFIER_TOKEN -p 8080:8080 ghcr.io/jadpo-dev/verifier
```

It checks a program in its sandbox has no network, then listens on port 8080.

**If it can't listen**, give another address, or stop what holds that one:

```sh
jadpo package verify --serve --listen 127.0.0.1:8081
```

It then answers at that address instead.

## Related

- [The verifier](../language/11-integrations.md#the-verifier)
- [`jadpo package verify`](../language/14-tooling.md#jadpo-package-verify)
