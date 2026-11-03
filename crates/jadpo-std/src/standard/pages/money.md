# Money

An amount of money: a whole number of a currency's smallest units, and the currency.

Use it for every amount of money. Never hold money in a `Decimal`: this type
can't lose a penny to rounding, and it can't add pounds to euros. For an amount
with whether tax is included, use `Price`. To fix the currency, build your own
type on it.

```jadpo
money.from_minor(1050, Currency.gbp)
```

- `money.from_minor(1050, Currency.gbp)` is £10.50, since it takes pence.
  `money.of` takes a decimal amount instead.
- A percentage scales it: `price * 120%` adds 20%. To share it out, use
  `money.split`, which loses nothing: £10.00 three ways is £3.34, £3.33 and
  £3.33. `price / 3` is refused.
- `<` compares two amounts in one currency. With two different currencies the
  compiler refuses it, or, if a currency is only known while running, it fails with
  `currency_mismatch`.
- A narrower type is often better: `type Sterling = Money { currency: Currency.gbp min: 0 }`.

# Currency

A currency, by its three-letter code: `GBP`.

Use it beside an amount, or to store which currency a customer pays in. An amount
in a currency is a `Money`, which already holds one.

```jadpo
Currency.gbp
```

- Each value is its code in lower case. A code that is a reserved word takes a
  trailing `_`: `Currency.all_` is the Albanian lek.
- It lists active currencies. Old currencies you already hold, such as Croatian
  kuna, are still read, but gold (`XAU`) and made-up codes aren't.

# ExchangeRate

A rate between two currencies at a moment in time.

Use it to record the rate you were given and to convert: `money.convert` applies
one. Jadpo doesn't fetch rates, so your app gets them from a provider.

```jadpo
ExchangeRate { from: Currency.gbp to: Currency.eur rate: 1.17 at: "2026-10-18T09:00:00Z" }
```

- It holds when the rate applied, so keep it with what you converted to show
  later how a figure was reached.

# Price

An amount, and whether it includes tax.

Use it for what you charge. It stops a price with tax being taken for one
without. For a plain amount, such as a refund or a balance, use `Money`. For a
price per kilo, use `UnitPrice`.

```jadpo
Price { amount: money.from_minor(1050, Currency.gbp) includes_tax: true }
```

- The amount can't be below zero.
- `text.format` shows only the amount, `£10.50`. It doesn't say whether tax is
  included, so add that wording yourself.

# UnitPrice

A price per amount of something, such as £2.50 per kilo.

Use it for goods sold by weight, volume, length or area, as UK shelf labels show
them. For the price of a thing you count, use `Price`.

```jadpo
UnitPrice.per_mass { price: money.from_minor(250, Currency.gbp) per: 1kg }
```

- Pick the variant by what you measure: `per_mass`, `per_volume`, `per_length`
  or `per_area`.
- Neither the price nor the amount can be below zero.

# TaxRate

A rate of tax, as a percentage that can't be below zero: `20%`.

Use it for VAT or sales tax. For any other percentage, use `Percentage`. For a
discount, use `Discount`.

```jadpo
TaxRate(20%)
```

# Discount

An amount off a price, or a percentage off it.

Use it for promotions and vouchers. `money.apply_discount` takes one off a
`Price`. For a price itself, use `Price`.

```jadpo
Discount.percentage_off { percentage: 20% }
```

- The other variant is `Discount.amount_off { amount: ... }`.
- An amount off can't be below zero, and a percentage off runs from 0% to 100%.

# BasisPoints

A count of hundredths of a percent: 25 basis points is 0.25%.

Use it for rate moves and fees in finance, where people talk in basis points. For
an ordinary percentage, use `Percentage`.

```jadpo
BasisPoints(25)
```

- It is a whole number and can be below zero. In JSON it is a plain number.

# InterestRate

A yearly rate of interest, and the basis it is quoted on: `5.5% APR`.

Use it for savings and loan rates. It keeps APR and AER from being confused. For
a plain percentage, use `Percentage`.

```jadpo
InterestRate { rate: 5.5% basis: InterestRateBasis.apr }
```

- The rate can be below 0%, as central banks' have been.

# PaymentCardNumber

A payment card's full number, checked by its check digit.

Use it only for the moment you pass a number on to a payment provider. To keep
something about a card, use `MaskedCardNumber`, which `money.mask` makes from
this one.

```jadpo
PaymentCardNumber("4242424242424242")
```

- `text.format` shows only the last four digits: `•••• 4242`.
- Write it with no spaces. Remove any a person types before you build one.

# MaskedCardNumber

What you may keep of a card once its number is gone: the brand and the last four digits.

Use it to show "Visa ending 4242" on an account page. To charge a card, your
payment provider's own token does that, not this.

```jadpo
MaskedCardNumber { brand: CardBrand.visa last_four: "4242" }
```

- `money.mask` makes one from a `PaymentCardNumber` and fills in the brand.
- `brand` is `none` when no listed card scheme issued the card.

# CardBrand

A card scheme: `visa`, `mastercard`, `american_express` and so on.

Use it to show which logo goes beside a card, or to accept only certain schemes.
`money.mask` tells it from a card's number, so you rarely set it yourself.

```jadpo
CardBrand.visa
```

# CardBin

The first 6 or 8 digits of a card number, which name its issuer.

Use it to look up a card's issuer or country, as fraud checks do. For the whole
number, use `PaymentCardNumber`.

```jadpo
CardBin("424242")
```

# CardExpiry

The month a payment card runs to, written `2028-12`.

Use it beside a card's details on a payment form. For any other month, use
`YearMonth`.

```jadpo
CardExpiry { year: 2028 month: 12 }
```

- `text.format` shows it as a card prints it, `12/28`, but `text.parse` reads only
  `2028-12`.
- The card is good until the end of that month.

# PaymentReference

The reference a payer sees on a UK bank payment: up to 18 characters.

Use it on Bacs and Faster Payments transfers, where banks allow only capitals,
digits, spaces and `. & / -`. For your own invoice's number, use `InvoiceNumber`.

```jadpo
PaymentReference("INV-2026/0042")
```

- It refuses lower case, so upper-case your text first.

# InvoiceNumber

An invoice's number: up to 200 characters, as your own system writes it.

Use it for the number on an invoice, such as `INV-2026-0042`. For what a bank
payment says, use `PaymentReference`.

```jadpo
InvoiceNumber("INV-2026-0042")
```

- It can't start or end with a space, or hold line breaks.

# Iban

An international bank account number, with its check digits: `GB29NWBK60161331926819`.

Use it for bank accounts in countries that use IBANs. To hold any kind of account,
use `BankAccount`, which can contain one.

```jadpo
Iban("GB29NWBK60161331926819")
```

- It has to be upper case with no spaces, so tidy what a person types before you
  build one.
- `text.format` shows it in groups of four.

# Bic

A bank's identifier code, also called a SWIFT code: 8 or 11 characters.

Use it to name a bank in an international payment. For the account at that bank,
use `Iban`.

```jadpo
Bic("DEUTDEFF")
```

# UkSortCode

A UK bank's sort code, as six digits: `089999`.

Use it beside a `UkAccountNumber`. For the two together, use `BankAccount`.

```jadpo
UkSortCode("089999")
```

- Write it with no dashes. `text.format` adds them: `08-99-99`.

# UkAccountNumber

A UK bank account number: eight digits, with any leading zeros kept.

Use it beside a `UkSortCode`. For the two together, use `BankAccount`.

```jadpo
UkAccountNumber("66374958")
```

- It must have all eight digits, so keep any leading zeros.

# BankAccount

A bank account to pay into or take from, in any country.

Use it when an account could be from anywhere. It holds a UK sort code and account
number, an IBAN, or another country's details, and you choose which you were given.

```jadpo
BankAccount.uk { sort_code: "089999" account_number: "66374958" }
```

- The others are `BankAccount.iban { iban: ... }` and `BankAccount.other { country:
  ... bank_code: ... account_number: ... }`.
- `text.format` shows only the last four digits, `•••• 4958`.

# AbaRoutingNumber

A US bank's routing number: nine digits with a check digit.

Use it to name a bank for US ACH and wire transfers. For UK accounts, use
`UkSortCode`.

```jadpo
AbaRoutingNumber("021000021")
```

# Lei

A Legal Entity Identifier, the global ID of a company in financial reporting.

Use it to name a company in regulatory or finance data. For a UK company's own
number, use `UkCompanyNumber`.

```jadpo
Lei("506700GE1G29325QX363")
```

- It is 20 characters in capitals, and it checks its own last two digits.

# Isin

A securities identification number, naming a share or a bond: `US0378331005`.

Use it to name a tradable security. For a company, use `Lei`.

```jadpo
Isin("US0378331005")
```

- It is 12 characters in capitals, and it checks its own last digit.

# InterestRateBasis

What an interest rate is quoted as: `aer`, `apr` or `flat`.

Use it as part of an `InterestRate`, which most apps want instead. `aer` is for
savings, `apr` for borrowing, and `flat` is a rate on the sum first lent.

```jadpo
InterestRateBasis.apr
```
