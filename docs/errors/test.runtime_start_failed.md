# `test.runtime_start_failed`

The tests couldn't start, because the machine couldn't give `jadpo test` the thread with a large stack they run on, usually for lack of memory.

## Why Jadpo refuses it

`jadpo test` runs the tests in the interpreter, on a thread of its own
with a large stack, so code that recurses deeply runs as it would in a
deployable. The operating system makes that thread when the run starts.
When it can't, because memory is short or the machine allows no more
threads, no test runs, and the reason ends with what the operating
system said.

Nothing in your project is wrong, and no test failed. Jadpo won't run
the tests on a smaller stack instead, where a test could fail only
because of where it ran; what to close or free on your machine is yours
to decide.

## What it looks like

A project with one test, of `draft` in todos.jadpo:

```jadpo check project=e-test-runtime-start file=todos.jadpo hidden
type TodoTitle = Text {
    min_length: 1
    max_length: 200
}

type Priority = Int {
    min: 1
    max: 9
}

type TodoDraft = Object {
    title: TodoTitle
    priority: Priority
}

function draft(title: TodoTitle) -> TodoDraft {
    return TodoDraft {
        title: title
        priority: 3
    }
}
```

todos.test.jadpo:

```jadpo check project=e-test-runtime-start file=todos.test.jadpo
test "a new draft has normal priority" {
    var fresh: TodoDraft = draft("Buy milk")
    assert fresh == TodoDraft {
        title: "Buy milk"
        priority: 3
    }
}
```

```sh
jadpo test .
```

A machine with memory to spare always makes the thread, so no ordinary
run raises this error. This output is copied rather than checked: it is
`jadpo test` on a Mac, run by a `jadpo` built from this checkout to ask
for a 4 PiB stack, more than any machine can give:

```text
error: The interpreter couldn't start
  reason: Tests run on a thread with a large stack, and starting it failed: Resource temporarily unavailable (os error 35).
  next: Free some memory, then run `jadpo test` again (human)
  rule: test.runtime_start_failed
  help: https://jadpo.dev/docs/errors/test.runtime_start_failed
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which programs on your machine are stopped to make room.

On Linux the operating system's words and number differ, as each system
names its own errors.

## How to fix it

**If other programs are using the memory**, close some, then run the
tests again:

```sh
jadpo test .
```

The thread starts once the machine has room for it, and the tests run.

## Related

- [`jadpo test`](../language/14-tooling.md#jadpo-test)
