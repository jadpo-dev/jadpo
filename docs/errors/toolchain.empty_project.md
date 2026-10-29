# `toolchain.empty_project`

```jadpo check project=e-empty file=notes.jadpo expect=toolchain.empty_project
// Nothing declared yet.
```

```text output
error: This project declares nothing
  reason: Its one `.jadpo` file holds no declarations, only comments or nothing at all.
  next: Add a declaration, such as a `type`, `function` or `action` (agent)
  rule: toolchain.empty_project
  help: docs/errors/toolchain.empty_project.md

check failed: 1 error, 1 file
```
