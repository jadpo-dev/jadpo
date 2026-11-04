# IataAirport

An airport's three-letter code, as tickets print it: `LHR`.

Use it for flights and passengers. For air traffic control's four-letter code, use
`IcaoAirport`. For shipping ports, use `UnLocode`.

```jadpo
IataAirport("LHR")
```

# IcaoAirport

An airport's four-letter code, as air traffic control uses it: `EGLL`.

Use it for flight operations. For the code on a ticket, use `IataAirport`.

```jadpo
IcaoAirport("EGLL")
```

# IataAirline

An airline's two-character code: `BA`, `U2`.

Use it to name an airline. With a flight's number, use `FlightNumber`.

```jadpo
IataAirline("BA")
```

# FlightNumber

A flight's code: `BA123`.

Use it for a passenger's flight. For the airline alone, use `IataAirline`.

```jadpo
FlightNumber { airline: "BA" number: 123 suffix: none }
```

- The number runs from 1 to 9999, and it has no leading zeros, so `BA0123` is refused.

# CabinClass

The cabin a seat is sold in: `economy`, `premium_economy`, `business` or `first`.

Use it for a booking or a fare.

```jadpo
CabinClass.economy
```

# BookingReference

A booking's six-character reference: `X7K9QZ`.

Use it to look up a flight booking. For a payment's own reference, use `PaymentReference`.

```jadpo
BookingReference("X7K9QZ")
```
