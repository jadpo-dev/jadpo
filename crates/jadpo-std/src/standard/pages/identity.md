# VatNumber

A VAT registration number, with its country's code at the front: `GB980780684`.

Use it for a business's VAT number on invoices. For a taxpayer's number other than
VAT, use `TaxId`.

```jadpo
VatNumber { country: Country.gb number: "980780684" }
```

- Greek numbers start `EL`, not `GR`, as the EU's VIES service writes them.
- It checks each country's form and check digit, but it doesn't ask a tax office
  whether the number is registered.

# TaxId

A taxpayer's identification number, with the country that issued it.

Use it for tax numbers from any country, such as in tax residence forms. For a
known UK or US number, `UkUtr`, `UkNationalInsuranceNumber` or `UsSsn` checks it
more closely.

```jadpo
TaxId { country: Country.gb number: "AB123456C" }
```

- `text.format` shows only the last four characters, `•••• 456C`.

# UkUtr

A UK Unique Taxpayer Reference: ten digits.

Use it for someone's or a company's reference with HMRC. For National Insurance,
use `UkNationalInsuranceNumber`. For a company at Companies House, use
`UkCompanyNumber`.

```jadpo
UkUtr("1234567890")
```

# UkCompanyNumber

A UK company's number at Companies House: `00445790` or `SC123456`.

Use it to name a registered UK company. Its tax reference is a `UkUtr` and its VAT
number a `VatNumber`.

```jadpo
UkCompanyNumber("SC123456")
```

- It is eight digits, or two capital letters and six digits. Leading zeros are kept,
  which is why it is text.

# UkNationalInsuranceNumber

A UK National Insurance number: `AB123456C`.

Use it for payroll and benefits. For tax numbers from any country, use `TaxId`.

```jadpo
UkNationalInsuranceNumber("AB123456C")
```

- Write it with no spaces. `text.format` adds them: `AB 12 34 56 C`.
- It refuses prefixes HMRC doesn't allocate, such as `QQ`, and `GB`.

# UsSsn

A US Social Security number: nine digits.

Use it only where US law asks for one. For a US company, use `UsEin`. For tax
numbers from any country, use `TaxId`.

```jadpo
UsSsn("123456789")
```

- Write it with no dashes. `text.format` shows only the last four: `•••-••-6789`.
- It refuses numbers that were never issued, such as those starting `000` or `666`.

# UsEin

A US Employer Identification Number, for a business: nine digits.

Use it to name a US business with the IRS. For a person, use `UsSsn`.

```jadpo
UsEin("123456789")
```

- Write it with no dash. `text.format` adds it: `12-3456789`.

# PassportNumber

A passport's number, with the country that issued it.

Use it for travel and identity checks. To hold any document with its expiry date,
use `IdentityDocument`.

```jadpo
PassportNumber { country: Country.gb number: "123456789" }
```

- Write the number in capitals. It is checked against the country's form where the
  country publishes one.
- `text.format` shows only the last four characters.

# DrivingLicenceNumber

A driving licence's number, with the country that issued it.

Use it for car hire or motor insurance. To hold any document with its expiry date,
use `IdentityDocument`.

```jadpo
DrivingLicenceNumber { country: Country.gb number: "MCADI801206HZ9HM" }
```

- Write the number in capitals with no spaces. It is checked against the country's
  form where the country publishes one.
- `text.format` shows only the last four characters.

# IdentityDocument

An identity document: its kind, the country, its number and when it expires.

Use it for identity checks that accept several kinds of document. When you take only
one kind, `PassportNumber` or `DrivingLicenceNumber` says so.

```jadpo
IdentityDocument {
    kind: IdentityDocumentKind.passport
    country: Country.gb
    number: "123456789"
    expires: "2031-05-01"
}
```

- The number is checked against the form for that kind from that country.
- Give `expires: none` for a document with no expiry date.
- `text.format` shows only the last four characters.

# IdentityDocumentKind

What kind of identity document one is: `passport`, `identity_card`, `driving_licence` or `residence_permit`.

Use it as part of an `IdentityDocument`, or to list which kinds your app accepts.

```jadpo
IdentityDocumentKind.passport
```

# NhsNumber

An NHS number, the patient identifier in England, Wales and the Isle of Man: ten digits.

Use it to identify a patient. Write it with no spaces. `text.format` adds them:
`943 476 5919`. For a condition or a diagnosis, use `Icd10Code` or `SnomedConcept`.

```jadpo
NhsNumber("9434765919")
```

- It checks its own last digit, so a mistyped number is usually refused.
