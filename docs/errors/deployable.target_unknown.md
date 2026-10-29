# `deployable.target_unknown`

A deployable is built for `Target.native` or `Target.wasm`; Workers is
where WebAssembly runs:

```jadpo check project=e-deployable-target-unknown expect=deployable.target_unknown file=app.jadpo
application {
    deployables: {
        kiosk: {
            serves: [main]
            target: Target.workers
        }
    }
}
```

```text output
error: `Target.workers` isn't a target
 --> app.jadpo:5:21
  |
5 |             target: Target.workers
  |                     ^^^^^^^^^^^^^^ not a `Target`
  reason: A deployable is built for `Target.native`, a server, or `Target.wasm`, WebAssembly on Cloudflare Workers. Did you mean `Target.wasm`?
  next: Use `Target.wasm` instead (agent)
  rule: deployable.target_unknown
  help: docs/errors/deployable.target_unknown.md

check failed: 1 error, 1 file
```
