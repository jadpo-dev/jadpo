# PersonName

A person's name: a given name and, if they have one, a family name.

Use it wherever you store or show what someone is called, such as a customer
on an order or the person a letter is addressed to. It keeps the two parts
apart, so `text.format` can write them in the order the reader's language
expects. For the name someone signs in with, use `Username`. For one part on
its own, use `GivenName` or `FamilyName`.

```jadpo
PersonName { given: "Mary" family: "Somerville" }
```

- Some people have only one name, as Sukarno did. Give it as `given`, with
  `family: none`.
- It has no single text form, so `text.from` won't take it. Show it with
  `text.format(name, locale: locale)`, which gives "Mary Somerville" in
  English and puts the family name first in Japanese.

# GivenName

The name a person goes by, one line as they write it.

Use it for a first name on its own, such as in "Hello, Mary". When you hold a
person's whole name, use `PersonName`, which contains one. For your own kind
of name, such as a product's, declare a type on `Text`.

```jadpo
GivenName("Mary")
```

- It takes any script and spaces inside, such as "José María" or "太郎", up to
  200 characters. It refuses an empty name, spaces at either end and line
  breaks.

# FamilyName

A person's family name, one line as they write it.

Use it for a surname on its own, such as a list sorted by surname. When you
hold a person's whole name, use `PersonName`, which contains one.

```jadpo
FamilyName("van der Berg")
```

- It takes any script and spaces inside, up to 200 characters. It refuses an
  empty name, spaces at either end and line breaks.

# Username

A login handle: lower-case letters, digits, `_`, `.` and `-`, up to 64 of them.

Use it for the name someone picks to sign in or to be mentioned by. For what
they are actually called, use `PersonName`. For an address you can write to,
use `Email`.

```jadpo
Username("mary.somerville_1780")
```

- It refuses capitals rather than lowering them, so lower-case what a person
  types before you build one.

# Email

An email address, checked as one.

Use it for any address you send to or sign someone in with. To keep the name
shown beside the address, use `EmailWithName`.

```jadpo
Email("mary@example.com")
```

- Checking the form doesn't show that a mailbox exists. To find out, send a
  message to it.

# EmailWithName

An email address with the name shown beside it: `Mary Somerville <mary@example.com>`.

Use it for the sender or recipient of a message when the name should appear in
the reader's mail app. When you only need the address, use `Email`.

```jadpo
EmailWithName { name: "Mary Somerville" address: "mary@example.com" }
```

- `text.parse` also reads a bare `mary@example.com`, and gives it `name: none`.

# PhoneNumber

A phone number with its country code, written `+447400123456`.

Use it for any number you might call or text. It is stored in one
international form, so two people typing the same number differently can't
create two records. For the digits dialled after it, use `PhoneExtension`. For
a country's code on its own, use `CallingCode`.

```jadpo
PhoneNumber("+447400123456")
```

- It refuses a number written the local way, such as `07400123456`, because
  that doesn't say which country it is in. Ask for the country as well and put
  the `+44` on yourself.
- `text.format` shows the number the way its own country writes it, which for
  this one is `07400 123456`.

# PhoneExtension

An internal extension dialled after a phone number: up to 20 digits.

Use it beside a `PhoneNumber` for an office line's extension. The main number
goes in the `PhoneNumber`.

```jadpo
PhoneExtension("2047")
```

- It holds digits only: leave out any `x` or "ext".

# PostalAddress

A postal address in any country.

Use it for where to send a parcel or a letter. It fits every country, since
locality, region and postal code are only there when the country uses them. A
point on a map is a `Point`, and a property's own number in Great Britain is a
`Uprn`.

```jadpo
PostalAddress {
    lines: ["Buckingham Palace"]
    locality: "London"
    region: none
    postal_code: "SW1A 1AA"
    country: Country.gb
}
```

- `text.format` writes it the way its country lays out an address, and leaves
  the country line out for a reader in the same country.
- The postal code is checked against the country's own form, so a value with
  a French code and `Country.gb` is refused.

# PostalCode

A postal code or ZIP code, such as `SW1A 1AA`.

Use it on its own when you only need the code, such as for a delivery estimate.
Inside a full address, it is a field of `PostalAddress`, which also checks it
against the country.

```jadpo
PostalCode("SW1A 1AA")
```

- Built on its own, it knows no country, so it checks only the general form:
  capital letters, digits, spaces and `-`, up to 12 of them. Upper-case what a
  person types before you build one.

# Uprn

A Unique Property Reference Number, the identifier for a property in Great Britain.

Use it to refer to a building or plot in data from Ordnance Survey or local
government. To write to someone, use `PostalAddress`.

```jadpo
Uprn("100023336956")
```

- It is text, not a number, even though it holds only digits. It has up to 12
  digits and can't start with a zero.

# Nationality

A person's nationality, named by country code: `GB`.

Use it for the nationality on a passport or a form. For where someone or
something is, use `Country`, which holds the same codes and means something
different.

```jadpo
Nationality.gb
```

- Each value is its country code in lower case. A code that is a reserved word
  takes a trailing `_`: `Nationality.in_` is Indian.
