# `type.any_object_outside_input`

A type is written `AnyObject` anywhere but in a provider package's object type's field, or an operation's answer, an event or a reply holds one.

## Why Jadpo refuses it

Some providers take values of any shape the account chooses: an email
provider's template variables, a payment provider's metadata. A provider
package declares such a field as `AnyObject`, and your app gives it one of
its own object types, which goes out as that type's JSON.

That only works in one direction. Nothing records which type an
`AnyObject` was given, so its JSON can't be read back as one. So
`AnyObject` is written only as a field of a provider package's object type,
`AnyObject` or `AnyObject?`, and only an operation's `input:` may hold that
type. Your app's own types, signatures and variables name the type they
mean, and an operation's answer, an event, an email, an entity or a
webhook's reply never holds one.

## What it looks like

main.jadpo:

```jadpo check project=e-any-object expect=type.any_object_outside_input
type Greeting = Object {
    details: AnyObject
}
```

```text output
error: `AnyObject` is only the type of a provider package's input field
 --> main.jadpo:2:14
  |
2 |     details: AnyObject
  |              ^^^^^^^^^ not a package's input field
  reason: An `AnyObject` takes any of the application's own object types and sends it as its JSON, for a provider whose input takes values of any shape, such as an email's template variables. Only a field of a provider package's object type is one, written `AnyObject` or `AnyObject?`, for an operation's `input:` to hold; everywhere else, code names the object type it means.
  next: Name the application's own object type meant here instead (agent)
  rule: type.any_object_outside_input
  help: https://jadpo.dev/docs/errors/type.any_object_outside_input

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If it is in your app's own code**, name the object type the field holds:

```jadpo check project=e-any-object-fix
type GreetingName = Text {
    max_length: 80
}

type GreetingDetails = Object {
    name: GreetingName
}

type Greeting = Object {
    details: GreetingDetails
}
```

`details` now says what it holds, and a `Greeting` can be kept, sent to a
client or read back from JSON like any other object.

In a provider package, put the `AnyObject` on the object type
the operation's `input:` takes, and declare what the provider sends back in
types of their own. An answer that holds one, even through another type, is
this error at the operation's `output:`, and an event or a reply holding one
is too. [Types](../language/11-integrations.md#types) shows a package's
`AnyObject` field and the app's object given to it.

## Related

- [Other built-in types](../language/02-values-and-types.md#other-built-in-types)
- [Types](../language/11-integrations.md#types)
