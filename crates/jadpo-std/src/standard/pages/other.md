# UnNumber

A dangerous good's UN number: `UN1203`.

Use it for hazardous goods on a shipping document.

```jadpo
UnNumber("UN1203")
```

- Write it with no space. `text.format` adds one: `UN 1203`.

# Orcid

A researcher's ORCID iD: `0000-0002-1825-0097`.

Use it to identify a researcher. For their paper, use `Doi`.

```jadpo
Orcid("0000-0002-1825-0097")
```

- It keeps its hyphens, and it checks its own last character.

# CefrLevel

A language ability level, from `A1` to `C2`.

Use it for language courses and job adverts. For the language itself, use `Language`.

```jadpo
CefrLevel.b2
```

# Mpan

A UK electricity supply's 13-digit number.

Use it to switch or identify an electricity supply. For gas, use `Mprn`.

```jadpo
Mpan("1200012345679")
```

- It checks its own last digit, so a mistyped number is usually refused.

# Mprn

A UK gas supply's number: 6 to 10 digits.

Use it to switch or identify a gas supply. For electricity, use `Mpan`.

```jadpo
Mprn("1234567")
```

# EpcRating

A UK energy certificate's band: `A` to `G`.

Use it for a property's energy rating.

```jadpo
EpcRating.c
```

# Tenure

How land in England and Wales is held: `freehold`, `leasehold` or `commonhold`.

Use it for a property listing. A share of a freehold is `leasehold` here, with your own field
for the share.

```jadpo
Tenure.freehold
```

# LawfulBasis

Your lawful reason for using personal data, under GDPR: `consent`, `legitimate_interests` and so on.

Use it to record why you hold a person's data. Which one applies is a legal decision for you
to make, not something Jadpo checks.

```jadpo
LawfulBasis.consent
```

# Jurisdiction

Where a law applies: a country, or a part of one such as England.

Use it for where a rule or a licence applies. For where something is, use `Country` or
`CountrySubdivision` on their own.

```jadpo
Jurisdiction.subdivision { subdivision: CountrySubdivision.gb_eng }
```

- The other variant is `Jurisdiction.country { country: ... }`.

# ModelId

An AI model by its provider and name: `openai/gpt-4o`.

Use it to record which model made an answer or an `Embedding`.

```jadpo
ModelId { provider: "openai" name: "gpt-4o" }
```

- As text it needs its provider, so `gpt-4o` alone is refused.
