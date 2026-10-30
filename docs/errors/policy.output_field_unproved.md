# `policy.output_field_unproved`

A note's owner may call `find_note`, but only its reviewer may read the appraisal, and an output has one shape for everyone:

```jadpo check project=e-output-field-unproved expect=policy.output_field_unproved
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText
    appraisal: NoteText {
        policy: {
            read: [reviewer_id]
        }
    }

    policy: {
        read: [owner_id, reviewer_id]
    }
}

type NoteView = Output {
    pick: [Note.id, Note.text, Note.appraisal]
}

query find_note(note_id: Note.id) fails NoteNotFound -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteNotFound
}
```

```text output
error: `find_note` sends `Note.appraisal`, which not every caller may read
 --> main.jadpo:46:57
   |
46 | query find_note(note_id: Note.id) fails NoteNotFound -> NoteView {
   |                                                         ^^^^^^^^ carries `appraisal`
  reason: Anyone `Note`'s `read:` lets in may call `find_note`, the user `owner_id` points at among them, and only the user `reviewer_id` points at may read `appraisal`. An output has one shape for everyone, so a field some callers may not read can't be in it.
  next: Return an output without `appraisal` from `find_note` (agent)
  rule: policy.output_field_unproved
  help: https://jadpo.dev/docs/errors/policy.output_field_unproved

check failed: 1 error, 1 file
```

A value copied through a variable or another type keeps its field's rules, so the same appraisal built into an output by an action is refused there:

```jadpo check project=e-output-field-copied expect=policy.output_field_unproved
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText
    appraisal: NoteText {
        policy: {
            read: [reviewer_id]
        }
    }

    policy: {
        read: [owner_id, reviewer_id]
    }
}

type Assessment = Object {
    appraisal: Note.appraisal
}

type NoteView = Output {
    pick: [Note.id, Note.appraisal]
}

action show_note(note_id: Note.id) fails NoteNotFound -> NoteView {
    var note: Note = attempt Note.get(note_id)
    var assessment: Assessment = Assessment {
        appraisal: note.appraisal
    }
    return NoteView {
        id: note.id
        appraisal: assessment.appraisal
    }
}
```

```text output
error: `NoteView.appraisal` carries `Note.appraisal`, which not every caller of `show_note` may read
 --> main.jadpo:57:20
   |
57 |         appraisal: assessment.appraisal
   |                    ^^^^^^^^^^^^^^^^^^^^ from `Note.appraisal`
  reason: `show_note` reaches the `Note` it comes from for anyone its grants let in, the user `owner_id` points at among them, and only the user `reviewer_id` points at may read `appraisal`. What a caller is sent has one shape for everyone, and a value passed through a variable or another type keeps its field's rules.
  next: Leave `appraisal` out of `NoteView` (agent)
  rule: policy.output_field_unproved
  help: https://jadpo.dev/docs/errors/policy.output_field_unproved

check failed: 1 error, 1 file
```

A condition shows the field it tests, so a list of the notes whose
appraisal is "Excellent" is refused to their owners, who may not read it:

```jadpo check project=e-condition-field-unproved expect=policy.output_field_unproved
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText
    appraisal: NoteText {
        policy: {
            read: [reviewer_id]
        }
    }

    policy: {
        read: [owner_id, reviewer_id]
    }
}

type NoteView = Output {
    pick: [Note.id, Note.text]
}

query praised_notes() -> List<NoteView> {
    from: Note
    where: Note.appraisal == "Excellent"
    order: [Note.text asc]
    limit: 50
}
```

```text output
error: `praised_notes` filters on `Note.appraisal`, which not every caller may read
 --> main.jadpo:48:12
   |
48 |     where: Note.appraisal == "Excellent"
   |            ^^^^^^^^^^^^^^ tests `appraisal`
  reason: Anyone `Note`'s `read:` lets in may see the records `praised_notes` reads, the user `owner_id` points at among them, and only the user `reviewer_id` points at may read `appraisal`. Which records pass a condition shows each one's `appraisal`, so a query filters and orders only on fields every caller may read.
  next: Remove the condition on `Note.appraisal`, or test a field every caller may read (agent)
  rule: policy.output_field_unproved
  help: https://jadpo.dev/docs/errors/policy.output_field_unproved

check failed: 1 error, 1 file
```

A value made under a condition shows what the condition tests, so a verdict built from constants is refused when the `if` deciding it tests the appraisal:

```jadpo check project=e-output-field-decided expect=policy.output_field_unproved
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText
    appraisal: NoteText {
        policy: {
            read: [reviewer_id]
        }
    }

    policy: {
        read: [owner_id, reviewer_id]
    }
}

type Laurel = Output {
    praised: Bool
}

action judge_note(note_id: Note.id) fails NoteNotFound -> Laurel {
    var note: Note = attempt Note.get(note_id)
    if (note.appraisal == "Excellent") {
        return Laurel {
            praised: true
        }
    }
    return Laurel {
        praised: false
    }
}
```

```text output
error: `Laurel.praised` carries `Note.appraisal`, which not every caller of `judge_note` may read
 --> main.jadpo:50:22
   |
50 |             praised: true
   |                      ^^^^ decided by `Note.appraisal`
  reason: `judge_note` reaches the `Note` it comes from for anyone its grants let in, the user `owner_id` points at among them, and only the user `reviewer_id` points at may read `appraisal`. Which value is sent shows what decided it, so a value made under an `if`, a `match` or a loop, or kept by a filter, keeps the rules of the fields they test.
  next: Decide `praised` by what every caller of `judge_note` may read (agent)
  rule: policy.output_field_unproved
  help: https://jadpo.dev/docs/errors/policy.output_field_unproved

check failed: 1 error, 1 file
```

A failure raised on such a condition shows what it tests the same way:

```jadpo check project=e-failure-decided expect=policy.output_field_unproved
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText
    appraisal: NoteText {
        policy: {
            read: [reviewer_id]
        }
    }

    policy: {
        read: [owner_id, reviewer_id]
    }
}

type NoteView = Output {
    pick: [Note.id, Note.text]
}

type NotPraised = Failure {
    kind: FailureKind.conflict
    message: "Only a praised note can be pinned."
}

action pin_note(note_id: Note.id) fails NoteNotFound, NotPraised -> NoteView {
    var note: Note = attempt Note.get(note_id)
    if (note.appraisal != "Excellent") {
        reject NotPraised
    }
    return NoteView {
        id: note.id
        text: note.text
    }
}
```

```text output
error: `pin_note` raises `NotPraised` on a condition that tests `Note.appraisal`, which not every caller may read
 --> main.jadpo:54:9
   |
54 |         reject NotPraised
   |         ^^^^^^^^^^^^^^^^^ decided by `Note.appraisal`
  reason: `pin_note` reaches the `Note` it comes from for anyone its grants let in, the user `owner_id` points at among them, and only the user `reviewer_id` points at may read `appraisal`. Whether `NotPraised` is raised shows what the condition tested, so a failure is raised only on what every caller may read.
  next: Test a field every caller may read, or raise `NotPraised` only where callers who may read `appraisal` reach (agent)
  rule: policy.output_field_unproved
  help: https://jadpo.dev/docs/errors/policy.output_field_unproved

check failed: 1 error, 1 file
```

Which record a `get` finds shows the value it is given, so a citation only the reviewer may know can't find its record for the note's owner:

```jadpo check project=e-lookup-field-unproved expect=policy.output_field_unproved
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Citation {
    id: Uuid
    title: NoteText

    policy: {
        read: [Access.authenticated]
    }
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    citation_id: Citation.id {
        on_delete: OnDelete.restricted
        policy: {
            read: [reviewer_id]
        }
    }
    text: NoteText

    policy: {
        read: [owner_id, reviewer_id]
    }
}

type CitationView = Output {
    pick: [Citation.id, Citation.title]
}

action note_citation(note_id: Note.id) fails NoteNotFound, CitationNotFound -> CitationView {
    var note: Note = attempt Note.get(note_id)
    var citation: Citation = attempt Citation.get(note.citation_id)
    return CitationView {
        id: citation.id
        title: citation.title
    }
}
```

```text output
error: `note_citation` finds a `Citation` by `Note.citation_id`, which not every caller may read
 --> main.jadpo:58:51
   |
58 |     var citation: Citation = attempt Citation.get(note.citation_id)
   |                                                   ^^^^^^^^^^^^^^^^ from `Note.citation_id`
  reason: `note_citation` reaches the `Note` it comes from for anyone its grants let in, the user `owner_id` points at among them, and only the user `reviewer_id` points at may read `citation_id`. Whether a `Citation` is found by it, and what it holds, shows what `citation_id` holds, so a value used to find a record keeps its field's rules, as one sent in an output does.
  next: Find it by a value every caller of `note_citation` may read (agent)
  rule: policy.output_field_unproved
  help: https://jadpo.dev/docs/errors/policy.output_field_unproved

check failed: 1 error, 1 file
```

An include shows the reference it follows the same way:

```jadpo check project=e-include-reference-unproved expect=policy.output_field_unproved
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Citation {
    id: Uuid
    title: NoteText

    policy: {
        read: [Access.authenticated]
    }
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    citation_id: Citation.id {
        on_delete: OnDelete.restricted
        policy: {
            read: [reviewer_id]
        }
    }
    text: NoteText

    policy: {
        read: [owner_id, reviewer_id]
    }
}

type CitationView = Output {
    pick: [Citation.id, Citation.title]
}

type NoteWithCitation = Output {
    pick: [Note.id, Note.text]
    citation: CitationView
}

query note_with_citation(note_id: Note.id) fails NoteNotFound -> NoteWithCitation {
    from: Note
    where: Note.id == note_id
    missing: NoteNotFound
    include: {
        citation: Note.citation_id
    }
}
```

```text output
error: `note_with_citation` includes the `Citation` each `Note` refers to through `citation_id`, which not every caller may read
 --> main.jadpo:66:9
   |
66 |         citation: Note.citation_id
   |         ^^^^^^^^ through `citation_id`
  reason: Anyone `Note`'s `read:` lets in may see the records `note_with_citation` reads, the user `owner_id` points at among them, and only the user `reviewer_id` points at may read `citation_id`. Which record an include reads shows the reference it follows, so a query includes only through references every caller may read.
  next: Remove the include, or give `Note.citation_id`'s `read:` everyone who may read a `Note` (agent)
  rule: policy.output_field_unproved
  help: https://jadpo.dev/docs/errors/policy.output_field_unproved

check failed: 1 error, 1 file
```

A caller may change a record they may not read, so the record an update gives back is proved against its `read:`:

```jadpo check project=e-update-record-unproved expect=policy.output_field_unproved
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    editor_id: User.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText
    summary: NoteText

    policy: {
        read: [owner_id]
        update: [owner_id, editor_id]
    }
}

type SummaryView = Output {
    pick: [Note.id, Note.summary]
}

action edit_note(note_id: Note.id, revised: NoteText) fails NoteNotFound -> SummaryView {
    var note: Note = attempt Note.update(note_id) {
        set: {
            text: revised
        }
    }
    return SummaryView {
        id: note.id
        summary: note.summary
    }
}
```

```text output
error: `SummaryView.summary` carries `Note.summary`, which not every caller of `edit_note` may read
 --> main.jadpo:51:18
   |
51 |         summary: note.summary
   |                  ^^^^^^^^^^^^ from `Note.summary`
  reason: `edit_note` reaches the `Note` it comes from for anyone its grants let in, the user `editor_id` points at among them, and only the user `owner_id` points at may read `summary`. What a caller is sent has one shape for everyone, and a value passed through a variable or another type keeps its field's rules.
  next: Leave `summary` out of `SummaryView` (agent)
  rule: policy.output_field_unproved
  help: https://jadpo.dev/docs/errors/policy.output_field_unproved

check failed: 1 error, 1 file
```

What a write stores is read later, from any request, by everyone who may read the field it is stored in, so a value is stored only where all of them may read it. Here the reviewer copies the appraisal into the summary, which the note's owner may read too:

```jadpo check project=e-stored-field-unproved expect=policy.output_field_unproved
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    summary: NoteText
    appraisal: NoteText {
        policy: {
            read: [reviewer_id]
        }
    }

    policy: {
        read: [owner_id, reviewer_id]
        update: [reviewer_id]
    }
}

action publish_appraisal(note_id: Note.id) fails NoteNotFound -> Note {
    var note: Note = attempt Note.get(note_id)
    return Note.update(note) {
        set: {
            summary: note.appraisal
        }
    }
}
```

```text output
error: `publish_appraisal` stores `Note.appraisal` in `Note.summary`, which not everyone who may read `summary` may read
 --> main.jadpo:47:22
   |
47 |             summary: note.appraisal
   |                      ^^^^^^^^^^^^^^ from `Note.appraisal`
  reason: Everyone who may read `Note.summary`, the user `owner_id` points at among them, reads what is stored there, from any request, and only the user `reviewer_id` points at may read `appraisal`. A value stored in a field keeps its own field's rules, as one sent in an output does, so it is stored only where everyone who may read it may read the value.
  next: Leave `appraisal` out of `summary`, or store it in a field only those who may read `appraisal` may read (agent)
  rule: policy.output_field_unproved
  help: https://jadpo.dev/docs/errors/policy.output_field_unproved

check failed: 1 error, 1 file
```

A copy between fields the same callers read stays allowed: the reviewer's draft, which only the reviewer may read, becomes the appraisal, which only the reviewer may read too. So does a copy into a record that refers to the one it came from, or into one of the same tenant, when everyone who may read the field may read the value there:

```jadpo check project=stored-field-same-readers
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    summary: NoteText
    draft: NoteText {
        policy: {
            read: [reviewer_id]
        }
    }
    appraisal: NoteText {
        policy: {
            read: [reviewer_id]
        }
    }

    policy: {
        read: [owner_id, reviewer_id]
        update: [reviewer_id]
    }
}

action finish_review(note_id: Note.id) fails NoteNotFound -> Note {
    var note: Note = attempt Note.get(note_id)
    return Note.update(note) {
        set: {
            appraisal: note.draft
        }
    }
}
```

A subscriber's action stores what its event carries, and the event carries what every `emit` of it put there. Here one of the two actions that share a note puts its appraisal, which only the reviewer may read, in the event, and the bulletin the subscriber posts is read by anyone signed in:

```jadpo check project=e-event-stored-unproved expect=policy.output_field_unproved
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    circulated: Bool
    summary: NoteText
    appraisal: NoteText {
        policy: {
            read: [reviewer_id]
        }
    }

    policy: {
        read: [Access.authenticated]
        update: [reviewer_id]
    }
}

entity Bulletin {
    id: Uuid
    blurb: NoteText

    policy: {
        create: [Access.system]
        read: [Access.authenticated]
    }
}

type NoteCirculated = Event {
    note_id: Note.id
    blurb: NoteText
}

action circulate_summary(note_id: Note.id) fails NoteNotFound -> Unit {
    var note: Note = attempt Note.update(note_id) {
        set: {
            circulated: true
        }
    }
    emit NoteCirculated {
        note_id: note.id
        blurb: note.summary
    }
}

action circulate_appraisal(note_id: Note.id) fails NoteNotFound -> Unit {
    var note: Note = attempt Note.update(note_id) {
        set: {
            circulated: true
        }
    }
    emit NoteCirculated {
        note_id: note.id
        blurb: note.appraisal
    }
}

action post_bulletin(blurb: NoteText) -> Unit {
    var bulletin: Bulletin = Bulletin.create {
        set: {
            blurb: blurb
        }
    }
    assert bulletin.blurb == blurb
}

subscriber post_circulated {
    on: NoteCirculated
    run: post_bulletin(event.blurb)
}
```

```text output
error: `post_circulated` passes `Note.appraisal` to `post_bulletin`, which stores it in `Bulletin.blurb`, though not everyone who may read `blurb` may read it
 --> main.jadpo:91:24
   |
91 |     run: post_bulletin(event.blurb)
   |                        ^^^^^^^^^^^ an `emit` filled this from `Note.appraisal`
  reason: `post_bulletin` stores what it is given here in `Bulletin.blurb`, and everyone who may read `Bulletin.blurb`, anyone signed in among them, reads what is stored there, from any request, and only the user `reviewer_id` points at may read `appraisal`. An event carries what each `emit` of it put there away from the record it came from, so its subscriber stores it only where everyone who may read it may read `appraisal` of any `Note`.
  next: Pass the event's ids and have `post_bulletin` read what it stores, or give `blurb`'s `read:` only those who may read `appraisal` (agent)
  rule: policy.output_field_unproved
  help: https://jadpo.dev/docs/errors/policy.output_field_unproved

check failed: 1 error, 1 file
```

With only the summary, which anyone signed in may read, in the event, the bulletin may hold it. A value an event carries has left its record, which may change before the delivery, so it is proved as one from any record of its entity, or from a record of the event's tenant when the event's tenant field came from that record, since the subscriber is limited to that tenant (area 12, "Subscribers"):

```jadpo check project=event-stored-allowed
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    circulated: Bool
    summary: NoteText
    appraisal: NoteText {
        policy: {
            read: [reviewer_id]
        }
    }

    policy: {
        read: [Access.authenticated]
        update: [reviewer_id]
    }
}

entity Bulletin {
    id: Uuid
    blurb: NoteText

    policy: {
        create: [Access.system]
        read: [Access.authenticated]
    }
}

type NoteCirculated = Event {
    note_id: Note.id
    blurb: NoteText
}

action circulate_summary(note_id: Note.id) fails NoteNotFound -> Unit {
    var note: Note = attempt Note.update(note_id) {
        set: {
            circulated: true
        }
    }
    emit NoteCirculated {
        note_id: note.id
        blurb: note.summary
    }
}

action post_bulletin(blurb: NoteText) -> Unit {
    var bulletin: Bulletin = Bulletin.create {
        set: {
            blurb: blurb
        }
    }
    assert bulletin.blurb == blurb
}

subscriber post_circulated {
    on: NoteCirculated
    run: post_bulletin(event.blurb)
}
```

A webhook's reply action runs as the system, which finds every record its
grants name, and its reply goes back to whoever sent the webhook, so a
reply that counts or tests for records only the system may read shows
whether they are there, and so does whether it fails. Each
is proved as an output to `Access.public`:

```jadpo check project=e-reply-shows-records file=app.jadpo hidden
use support

application {
    connections: {
        phone: {
            package: AcmeVoice.v1
            domains: [support]
            credential: config.phone_key
            host: config.phone_address
            webhooks: {
                secret: config.phone_auth_token
                replies: {
                    incoming_call: support.answer_call
                }
            }
        }
    }
}
```

```jadpo check project=e-reply-shows-records file=config.jadpo hidden
config {
    phone_address: Url
    phone_auth_token: Text {
        secret: true
    }
    phone_key: Text {
        secret: true
    }
}
```

```jadpo check project=e-reply-shows-records file=shared/providers/acme_voice/v1/package.jadpo hidden
/// Acme Voice's calls API.
package AcmeVoice.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-reply-shows-records file=shared/providers/acme_voice/v1/provider.jadpo hidden
/// Acme Voice's API, which signs its webhooks as Twilio does.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.bearer
    webhooks: {
        check: SignatureScheme.twilio
        event_id: "CallSid"
        event_type: "CallStatus"
    }
}
```

```jadpo check project=e-reply-shows-records file=shared/providers/acme_voice/v1/types/call_reply.jadpo hidden
/// What a caller hears.
public type CallReply = Object {
    spoken: Text
    hang_up: Bool {
        wire_name: "hangUp"
    }
}
```

```jadpo check project=e-reply-shows-records file=shared/providers/acme_voice/v1/events/incoming_call.jadpo hidden
/// A call is ringing, and waits to be told what to do.
public type IncomingCall = Event {
    wire_name: "ringing"
    reply: CallReply
    occurred_at: Instant
    call_sid: Text {
        wire_name: "CallSid"
    }
    from: Text {
        wire_name: "From"
    }
}
```

```jadpo check project=e-reply-shows-records file=domains/support/entities/phone_log.jadpo hidden
/// A call the desk answered.
entity PhoneLog {
    id: Uuid
    call_sid: Text
    caller: Text
    rang_at: Instant

    policy: {
        create: [Access.system]
        read: [Access.system]
    }
}
```

```jadpo check project=e-reply-shows-records file=domains/support/queries/recent_calls.jadpo hidden
use support/entities { PhoneLog }

/// The last calls the desk answered.
query recent_calls() -> List<PhoneLog> {
    from: PhoneLog
    order: [PhoneLog.rang_at desc]
    limit: 10
}
```

```jadpo check project=e-reply-shows-records file=domains/support/types/call_count.jadpo hidden
/// How many calls the desk answered.
type CallCount = Int {
    min: 0
}
```

```jadpo check project=e-reply-shows-records file=domains/support/actions/count_calls.jadpo hidden
use support/types { CallCount }
use support/queries { recent_calls }

/// How many of the last calls the desk answered.
action count_calls() -> CallCount {
    return CallCount(list.count(recent_calls()))
}
```

```jadpo check project=e-reply-shows-records file=domains/support/actions/answer_call.jadpo migrate=initial hidden
use shared/providers/acme_voice

/// Answers a call Acme Voice rings through.
action answer_call(event: AcmeVoice.IncomingCall) -> AcmeVoice.CallReply {
    return AcmeVoice.CallReply {
        spoken: "Thanks for calling."
        hang_up: false
    }
}
```

```jadpo check project=e-reply-shows-records file=domains/support/actions/answer_call.jadpo expect=policy.output_field_unproved
use support/entities { PhoneLog }
use support/queries { recent_calls }
use shared/providers/acme_voice

/// Answers a call Acme Voice rings through, welcoming a caller back.
action answer_call(event: AcmeVoice.IncomingCall) -> AcmeVoice.CallReply {
    var earlier: List<PhoneLog> = recent_calls()
    var mut said: Text = "Thanks for calling."
    if (list.count(earlier) > 0) {
        said = "Welcome back."
    }
    return AcmeVoice.CallReply {
        spoken: said
        hang_up: false
    }
}
```

```text output
error: `answer_call`'s reply shows whether a `PhoneLog` is there, which not everyone may read
 --> domains/support/actions/answer_call.jadpo:12:12
   |
12 |     return AcmeVoice.CallReply {
   |            ^^^^^^^^^^^^^^^^^^^^^ decided by whether a `PhoneLog` is there
  reason: A webhook's reply action answers whoever sent the webhook, with its reply or a failure, so each is an output to `Access.public`, and only schedules and subscribers may read a `PhoneLog`. Which reply is sent shows whether the records it counted or tested are there, since the action runs as the system and finds every one.
  next: Decide the reply without counting or testing for a `PhoneLog` (agent)
  rule: policy.output_field_unproved
  help: https://jadpo.dev/docs/errors/policy.output_field_unproved

check failed: 1 error, 11 files
```
