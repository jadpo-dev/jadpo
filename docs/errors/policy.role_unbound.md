# `policy.role_unbound`

A role exists only where a membership stores it; declaring the enum gives it to nobody:

```jadpo check project=e-role-unbound expect=policy.role_unbound
type NoteRole = Enum {
    owner
}

entity Note {
    id: Uuid

    policy: {
        read: [NoteRole.owner]
    }
}
```

```text output
error: No membership holds a `NoteRole`
 --> main.jadpo:9:16
  |
9 |         read: [NoteRole.owner]
  |                ^^^^^^^^^^^^^^ a role nobody can hold
  reason: A role exists only where it is stored: in a membership entity whose `membership:` names a field typed `NoteRole` as its `role:`. Declaring the enum alone gives the role to nobody.
  next: Add a membership entity holding `NoteRole`, or name someone else in the grant (agent)
  rule: policy.role_unbound
  help: docs/errors/policy.role_unbound.md

check failed: 1 error, 1 file
```
