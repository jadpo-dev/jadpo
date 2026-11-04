# Country

A country, written as its two-letter code: `GB`.

Use it for where something or someone is, such as the country of an address.
For someone's citizenship, use `Nationality`. For a state or a region inside a
country, use `CountrySubdivision`.

```jadpo
Country.gb
```

- Each value is its code in lower case. A code that is a reserved word takes
  a trailing `_`: `Country.in_` is India.
- `text.format` can also write the three-letter code, `GBR`, with `form:
  CodeForm.alpha3`, or the number, `826`, with `form: CodeForm.numeric`.
- The withdrawn code `YU` is only read from old data, so don't create new ones.

# CountrySubdivision

A state, province or other part of a country, written as its code: `GB-ENG`, `US-CA`.

Use it when you need to know the region for certain, such as for state sales
tax. An address's `region` is free text, so use this type only when you want a
checked list. For the whole country, use `Country`.

```jadpo
CountrySubdivision.us_ca
```

- The name is the code in lower case with `_` for `-`: `CountrySubdivision.gb_eng`.
- The list comes from ISO 3166-2. It has over 5,000 values, so look a code up
  there, or use `jadpo help CountrySubdivision --json` and search.

# Continent

A continent, as the United Nations groups the world: `europe`, `asia`.

Use it for rough grouping, such as a sales report by continent. For anything
legal or tax-related, use `Country`, since the UN's groups aren't borders.

```jadpo
Continent.europe
```

- The Americas are one value, `Continent.americas`. There's no separate North
  or South America.

# UnLocode

A place in trade and transport: a country's code and three letters or digits, `GBLON`.

Use it for ports, freight terminals and other places in shipping documents. For
an airport, use `IataAirport`. For a street address, use `PostalAddress`.

```jadpo
UnLocode("GBLON")
```

- Write it with no space and in capitals. `text.format` shows `GB LON`, the way
  it is usually printed.

# CallingCode

A country's telephone code with its `+`: `+44`.

Use it for a dialling-code picker on a form. A whole number belongs in a
`PhoneNumber`, which already contains its code.

```jadpo
CallingCode("+44")
```

- It always has the `+`. `44` on its own is refused.

# Language

A language, written as its two-letter code: `en`.

Use it to say what language something is in, such as a document or a subtitle.
When the region matters as well, as with dates and numbers, use `Locale`, which
tells `en-GB` from `en-US`.

```jadpo
Language.en
```

- Each value is its code. A code that is a reserved word takes a trailing `_`:
  `Language.or_` is Odia.
- `text.format` can also write the three-letter code, `eng`, with `form:
  CodeForm.alpha3`.

# WritingScript

A writing system, written as its four-letter code: `Latn`, `Jpan`.

Use it when the script matters separately from the language, such as Serbian in
Latin or Cyrillic, or choosing a font. Most apps want `Language` or `Locale`.

```jadpo
WritingScript.latn
```

- The value's name is lower case, `WritingScript.latn`, but it is written `Latn`.

# TextDirection

Which way text runs: `ltr` or `rtl`.

Use it to tell a front end how to lay out a page. You rarely store one:
`locale.direction` gives it from a `Locale`.

```jadpo
TextDirection.right_to_left
```

- The values are `TextDirection.left_to_right` and
  `TextDirection.right_to_left`, written as HTML's `dir` writes them.

# Locale

A language and region together, which decides how dates and numbers look: `en-GB`.

Use it whenever you format something for a person, and to store which they
prefer. To say only which language a document is in, use `Language`.

```jadpo
Locale.en_gb
```

- Each value is its tag in lower case with `_` for `-`: `Locale.en_gb` is `en-GB`.
- `locale.from_accept_language` picks one from a browser's request header.

# Zone

A time zone, written as its name: `Europe/London`.

Use it wherever you turn an instant into a local time or the reverse, and to
store where a person or a shop is. A fixed gap from UTC with no daylight saving
rules is a `UtcOffset`.

```jadpo
Zone.europe_london
```

- Each value is its name in lower case with `_` for `/`: `Zone.america_new_york`.
- `GMT+1` isn't a zone. A city's name in this form is what keeps daylight saving
  correct.
