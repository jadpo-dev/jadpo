# `acceptance.missing`

Some changes can break someone else or widen who can do what, so a person
accepts them: an API break, a route opened to anyone, a caller added to a
grant, a new outside host, a decline of a security rule, a new domain, a
package installed from git. `jadpo check` compares the change with the
default branch at its merge base, and fails on each such change until an
acceptance the change adds lists it. Here an agent removed `priority` from
two outputs:

```text
error: removing an output field can break clients: main.TodoSummary.priority, main.TodoView.priority
 --> api/main.jadpo_record:31:1
   |
31 | output TodoSummary
   | ^^^^^^^^^^^^^^^^^^ ApiChange.output_field_removed
  reason: Clients that read it find it gone. So a person accepts it: no acceptance this change adds lists these as `ApiChange.output_field_removed`.
  next: A person accepts it: `jadpo acceptance new <purpose>` writes the draft for them (human)
  rule: acceptance.missing
  help: https://jadpo.dev/docs/errors/acceptance.missing
```

The agent runs `jadpo acceptance new remove_todo_priority`, which lists
both changes in `acceptances/`, and stops. A person writes why on the
`///` line above `acceptance`
([Acceptances](../language/18-records-and-manifest.md#acceptances)).
