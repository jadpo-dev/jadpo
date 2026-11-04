# Percentage

A share of something, written `20%`, where `20%` is the same as 0.20.

Use it for rates and shares. A percentage scales an amount: `price * 120%` adds 20%.
For tax, use `TaxRate`. For one thing compared with another, use `Ratio`.

```jadpo
Percentage(20%)
```

- It can be below 0% or above 100%, since prices fall and rise. To limit it, build your
  own type on it with `min:` and `max:`.
- `Percentage(ratio)` makes one from a `Decimal` ratio, so 0.25 becomes 25%.
- In JSON it is a string, `"20%"`, so a plain `20` or `0.2` is refused.

# Ratio

One number to another, as written: `16:9`.

Use it to record a proportion as people say it, such as a recipe's ratio. `16:9` and
`32:18` stay two different values. For a share of a whole, use `Percentage`. For
screens, use `AspectRatio`.

```jadpo
Ratio { numerator: 16.0 denominator: 9.0 }
```

# Fraction

An exact fraction as written: `3/4`.

Use it for numbers people say as fractions, such as a share in a property. `1/2` and
`2/4` stay two different values. For a percentage, use `Percentage`.

```jadpo
Fraction { numerator: 3 denominator: 4 }
```

- `denominator` must be at least 1, so a negative fraction has a negative numerator.

# Range

The values of one ordered type from a start up to, but not including, an end.

Use it for spans of your own type, such as a price band, and name the type inside it:
`Range<Grams>`. For days, use `DateRange`. For moments, use `InstantRange`.

```jadpo
var example: Range<CalendarDate> = Range { start: "2026-10-01" end: none }
assert range.contains(example, CalendarDate("2026-10-25"))
```

- Give `none` for an end with no limit, so the example runs from 1 October onwards.
- `range.contains`, `range.overlaps` and `range.intersection` work on it.

# DataSize

An amount of data: `5MB` or `4KiB`.

Use it for file sizes and storage limits. `MB` is a million bytes and `MiB` is 2 to
the 20. For data per second, use `Bitrate`.

```jadpo
DataSize(5MB)
```

- It is held in whole bytes, and `text.format` takes `unit:`.

# Bitrate

Data per second: `100Mbps`.

Use it for network and video speeds. A `DataSize` over a `Duration` gives one.

```jadpo
Bitrate(100Mbps)
```

- It counts bits, while `DataSize` counts bytes, so 8 Mbps moves 1 MB each second.

# Vector

A list of numbers, as an AI model's embedding holds them.

Use it for similarity search. To keep the numbers with the model that made them, use
`Embedding`. For a fixed length, build your own type with `dimensions:`.

```jadpo
[0.5, -0.25, 1.0]
```

- It holds from 1 to 16,000 numbers.

# Embedding

A list of numbers with the AI model that made it.

Use it to store model output, since vectors from different models can't be compared.
For the numbers alone, use `Vector`.

```jadpo
Embedding {
    model: ModelId { provider: "openai" name: "text-embedding-3-small" }
    vector: [0.5, -0.25, 1.0]
}
```
