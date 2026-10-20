# 27 · A guest house

A guest house in Snowdonia takes bookings for its rooms and confirms
each one by email, in English or in Welsh, in the language the guest
chose. It teaches email: an email as a type, its HTML and text written
beside it and checked against its fields, its subject worded like any
message, a layout that wraps every email, and sending as emitting, so
only a booking that was made is ever confirmed. Guests sign in with a
link, or a code, emailed to them: no passwords. It teaches magic-link
sign-in too, with routes and a sign-in email of the guest house's own,
and passkeys, with a way back in by email for a guest who loses them
all.

An email's words are templates: HTML and plain text with Jadpo inside
`{{ }}`. They read the email's fields, translate and format, and
nothing else, so what an email says is decided where it is sent, and a
reviewer reads its words in one place.

## What it teaches

1. **An email type.** `type X = EmailMessage { ... }` in a domain's `emails/` folder: its fields are what its templates write, and every email has `to:` and `locale:`. See `domains/lodging/emails/booking_confirmed.jadpo` and [area 16, Email](../../docs/language/16-built-in-services.md#email).
2. **Templates.** `booking_confirmed.html` and `booking_confirmed.txt` sit beside the type. `{{ value }}` writes a field, escaped in HTML; `translations.lodging.<message>(...)` and `text.format(...)` write in the email's language without `locale:`; `{{ if (...) }} ... {{ else }} ... {{ end }}` and `{{ for item in list }} ... {{ end }}` choose and repeat; an enum with wording writes its wording. Anything else is a compile error. See `domains/lodging/emails/booking_confirmed.html` and `domains/lodging/emails/booking_confirmed.txt`.
3. **The subject.** An email's member in the domain's language files is its subject line, its fields its placeholders, worded in every supported language. See `domains/lodging/translations/en_gb.jadpo` and `domains/lodging/translations/cy_gb.jadpo`.
4. **The layout.** `shared/emails/layout.html` and `layout.txt` wrap every email: `{{ content }}` is the email's own text, `{{ subject }}` its subject, `{{ locale.direction }}` `ltr` or `rtl`, and `{{ app.display_name }}` the application's name. See `shared/emails/layout.html` and [area 16, Level 3: branding](../../docs/language/16-built-in-services.md#built-in-emails).
5. **Sending is emitting.** `emit BookingConfirmed { ... }` keeps the email with the booking, and it is sent once the booking commits, rendered whole as it is sent. See `domains/lodging/actions/book_stay.jadpo`.
6. **Where it goes.** `email:` in `app.jadpo` names the connection every email goes through and who it is from; here an SMTP server, its password a secret setting given only to the connection. Changing provider changes `app.jadpo` alone. See `app.jadpo`, `config.jadpo` and [area 16, The SMTP driver](../../docs/language/16-built-in-services.md#the-smtp-driver).
7. **Partials.** `type Button = EmailPartial { ... }` in `shared/emails/`, its `.html` and `.txt` beside it, is a piece every domain's emails may include, as `{{ include button(label: ..., url: ...) }}` with each field by name, written in the including email's language. See `shared/emails/button.jadpo` and [area 16, Email](../../docs/language/16-built-in-services.md#email).
8. **Tests catch email.** A test sends nothing: each email is rendered once its booking commits and caught, so a template that couldn't be rendered fails the test, and `email.sent` lists what the test sent, each with its address, subject, HTML, text and links. See `domains/lodging/actions/book_stay.test.jadpo`.
9. **Signing in by email.** `sign_in: { magic_link: }` in `authentication.jadpo` sends a link and a six-digit code, kept only as verifiers, each used once and only the latest working; `principal` finds a guest by `Guest.email`, and `on_first_sign_in:` names the action that makes a guest the first time an address signs in. See `authentication.jadpo`, `app.jadpo`, `domains/lodging/actions/welcome_guest.jadpo` and [area 8, Magic links](../../docs/language/08-identity.md#magic-links).
10. **Routes of its own.** `generate_routes: false` makes no sign-in routes, and the guest house writes its own over `magic_links.request`, `.redeem` and `.redeem_code`, which keep every protection the generated ones have; `sessions.start` then signs the browser in. A `SignInToken` (the link's) and a `SignInCode` are held only by an `Input` or an `Object`, never kept or sent back. See `domains/lodging/routes/sign_in.jadpo`, `domains/lodging/actions/sign_in_with_link.jadpo` and `domains/lodging/types/link_given.jadpo`.
11. **A sign-in email of its own.** `email: lodging.SignInEmail` replaces Jadpo's: an `EmailMessage` that picks the link, the code and how long they work from `MagicLinkEmail`, filled in and sent by Jadpo, and including the same button. See `domains/lodging/emails/sign_in_email.jadpo`.
12. **Jadpo's own words in Welsh.** Jadpo words its failures and its sign-in email in English only, so `translations/cy_gb.jadpo` words every message of the features the app has on, under `jadpo: { }`. See `translations/cy_gb.jadpo` and [area 16, Built-in emails](../../docs/language/16-built-in-services.md#built-in-emails).
13. **Sign-in events.** Every sign-in publishes `SignedIn`, and every refusal `SignInRefused`, which a subscriber hears as the system; here it notes when a guest last signed in. See `domains/lodging/subscribers/note_sign_ins.jadpo` and `domains/lodging/actions/note_sign_in.jadpo`.
14. **Signing in in a route test.** The test asks for a link, reads the email from `email.sent`, and posts the token from the end of its link, as the guest's browser would. See `domains/lodging/routes/sign_in.test.jadpo`, and `domains/lodging/routes/my_stays.test.jadpo` for a guest's own bookings.
15. **Passkeys.** `passkey: {}` beside `magic_link:` lets a guest add passkeys and sign in with their phone's fingerprint or face, through seven routes Jadpo writes under `/auth/passkey`; only a public key is kept, in Jadpo's own table, which the migration `add_passkeys` creates. Its refusals are worded in Welsh too. See `authentication.jadpo`, `translations/cy_gb.jadpo` and [area 8, Passkeys](../../docs/language/08-identity.md#passkeys).
16. **Recovery is a magic link.** A guest who loses every device holding a passkey signs in by email and adds a new one, so `passkey:` needs `magic_link:` in the same `sign_in:`. A route test drives the passkey routes with `test_passkeys`, a software authenticator: Megan signs in with her phone, loses it, signs in with the emailed link and adds a passkey on her new one. See `domains/lodging/routes/sign_in.test.jadpo` and [area 8, In development and tests](../../docs/language/08-identity.md#in-development-and-tests).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/27-guest-house
target/release/jadpo test examples/27-guest-house
target/release/jadpo fmt --check examples/27-guest-house
```

- `check` ends with `checked: 50 files` and no errors.
- `test` needs PostgreSQL, and ends with `tested: 7 passed, 2 action tests, 5 route tests`.

## Try next

- In `booking_confirmed.html`, write the arrival date as it is, `{{ arrives_on }}`. `jadpo check` reports `email.template_value_invalid`: a date is shown to a guest formatted, in their language, with `text.format`.
  <!-- try {"file": "domains/lodging/emails/booking_confirmed.html", "replace": "<p>{{ booked_room }}</p>", "with": "<p>{{ booked_room }}, {{ arrives_on }}</p>", "check": ["email.template_value_invalid"]} -->
- In `booking_confirmed.txt`, shout the guest's name with `text.upper(guest_name)`. `jadpo check` reports `email.template_not_allowed`: a template only reads its fields, translates and formats, and anything else is worked out where the email is sent.
  <!-- try {"file": "domains/lodging/emails/booking_confirmed.txt", "replace": "{{ booked_room }}", "with": "{{ text.upper(guest_name) }}", "check": ["email.template_not_allowed"]} -->
- In `cy_gb.jadpo`, leave out the subject, `booking_confirmed`. `jadpo check` reports `translation.wording_missing`: an email has a subject in every language the guest house writes.
  <!-- try {"file": "domains/lodging/translations/cy_gb.jadpo", "replace": "    booking_confirmed: \"Eich arhosiad yn Nhŷ Gwyn, {guest_name}\"\n", "with": "", "check": ["translation.wording_missing"]} -->
- In `booking_confirmed.html`, give the button a `link:` in place of its `url:`. `jadpo check` reports `email.partial_invalid`: a partial is given each of its own fields, by name.
  <!-- try {"file": "domains/lodging/emails/booking_confirmed.html", "replace": "url: stay_page", "with": "link: stay_page", "check": ["email.partial_invalid"]} -->
- In `translations/cy_gb.jadpo`, leave out `sign_in_code_invalid`. `jadpo check` reports `translation.jadpo_missing`, naming it: a guest reading Welsh would be told their code was wrong in English.
  <!-- try {"file": "translations/cy_gb.jadpo", "replace": "        sign_in_code_invalid: \"Dydy'r cod hwnnw ddim yn iawn. Gwiriwch ef, neu gofynnwch am un newydd.\"\n", "with": "", "check": ["translation.jadpo_missing"]} -->
- Then read [28-lost-property](../28-lost-property/README.md), where a model drafts replies, finds items by meaning and streams answers.
