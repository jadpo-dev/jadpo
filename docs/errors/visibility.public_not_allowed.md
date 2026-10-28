# `visibility.public_not_allowed`

A domain offers other domains its types, entities, queries, libraries and
actions, and their tests, fixtures and seeds its fixtures; nothing else is
`public`:

```jadpo check project=e-public file=app.jadpo hidden
application {}
```

```jadpo check project=e-public file=domains/planning/routes/todos.jadpo expect=visibility.public_not_allowed
public routes Todos {
    prefix: /todos
}
```

```text output
error: A route group can't be `public`
 --> domains/planning/routes/todos.jadpo:1:1
  |
1 | public routes Todos {
  | ^^^^^^ not part of a domain's interface
  reason: A route group is a trigger: nothing calls it, so there is nothing to offer another domain; who reaches a route is decided by the route and its policy.
  next: Remove `public` (agent)
  rule: visibility.public_not_allowed
  help: docs/language/01-project-layout-and-modules.html#visibility-public-not-allowed

check failed: 1 error, 2 files
```
