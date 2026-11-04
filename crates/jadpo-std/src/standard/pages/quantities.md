# Length

A length or distance: `5km`, `45cm`, `3inch` or `2metre`.

Use it for any distance, in any unit. It keeps your sums correct across units, so
`1metre + 3cm` just works. For a box's three sides, use `Dimensions`.

```jadpo
Length(5km)
```

- `m` is minutes, so a metre is `metre`. Inches are `inch`.
- A `Length` times a `Length` gives an `Area`.
- `units.of(5, LengthUnit.mile)` builds one from a number, and `units.in` reads a
  number back.
- It is held in metres to 6 places.

# Area

An area: `12sqm` or `2ha`.

Use it for floor space or land. A `Length` times a `Length` gives one. For a drawn
shape on a map, use `Polygon`, which `geo.area` measures.

```jadpo
Area(12sqm)
```

- It is held in square metres to 6 places.

# Volume

A volume: `500ml` or `2litre`.

Use it for liquids and capacity. An `Area` times a `Length` gives one.

```jadpo
Volume(500ml)
```

- It is held in cubic metres to 9 places.

# Angle

An angle: `90deg` or `1.5rad`.

Use it for rotations and turns. For a compass direction from north, use `Bearing`.

```jadpo
Angle(90deg)
```

- It is held in radians to 9 places.

# Mass

A mass or weight: `250g`, `2.5kg` or `500mcg`.

Use it for weights of goods and doses. For a dose of medicine with a route and
timing, use `DosageInstruction`.

```jadpo
Mass(2.5kg)
```

- It is held in kilograms to 9 places, which is one microgram.

# Pressure

A pressure: `1013hPa`.

Use it for weather and machinery. For a person's blood pressure, use `BloodPressure`.

```jadpo
Pressure(1013hPa)
```

- It is held in pascals to 3 places.

# Energy

An amount of energy: `2kWh` or `250kcal`.

Use it for electricity used or food labels. For the rate at which energy is used, use
`Power`.

```jadpo
Energy(2kWh)
```

- It is held in joules to 3 places.

# Power

A rate of energy use: `3kW`.

Use it for how strong a heater or a motor is. For how much was used over time, use
`Energy`.

```jadpo
Power(3kW)
```

- It is held in watts to 3 places.

# Temperature

A temperature: `21degC`, `70degF` or `300kelvin`.

Use it for a reading on a thermometer. For a gap between two readings, use
`TemperatureDifference`.

```jadpo
Temperature(21degC)
```

- You can add a `TemperatureDifference` to one, and subtracting one from another gives
  a `TemperatureDifference`. Two temperatures can't be added.
- It is held in kelvin to 3 places.

# TemperatureDifference

The gap between two temperatures, so 5 °C warmer isn't the same as 5 °C.

Use it for "raise by 5 degrees". A `Temperature` minus a `Temperature` gives one, and
`units.of` makes one from a number.

```jadpo
var warmer: TemperatureDifference = 26degC - 21degC
assert warmer == units.of(5, TemperatureDifferenceUnit.kelvin)
```

- It is held in kelvin to 3 places.

# Speed

A speed: `30mph` or `100kmh`.

Use it for how fast something moves. A `Length` over a `Duration` gives one. For data
speed, use `Bitrate`.

```jadpo
Speed(30mph)
```

- It is held in metres per second to 6 places.

# Frequency

How often something happens per second: `50Hz` or `2.4GHz`.

Use it for mains, radio and processor speeds. For a heartbeat, use `HeartRate`. For
video, use `FrameRate`.

```jadpo
Frequency(50Hz)
```

- It is held in hertz to 3 places.

# Voltage

An electrical voltage: `230V`.

Use it for electrical equipment. For the flow of electricity, use `Current`.

```jadpo
Voltage(230V)
```

- It is held in volts to 6 places.

# Current

An electric current: `500mA`.

Use it for electrical equipment. For its pressure, use `Voltage`.

```jadpo
Current(500mA)
```

- It is held in amperes to 6 places.

# Illuminance

How much light falls on a surface: `500lx`.

Use it for lighting and sensors.

```jadpo
Illuminance(500lx)
```

- It is held in lux to 3 places.

# SoundLevel

How loud something is: `60dB`.

Use it for noise readings. Decibels aren't a linear scale, so two levels can be
compared but not added.

```jadpo
SoundLevel(60dB)
```

- It is held in decibels to 2 places.

# Concentration

A mass in a volume: `5mgL` or `1gL`.

Use it for how much of something is dissolved, such as in lab results. A `Mass` over a
`Volume` gives one. For blood sugar, use `BloodGlucose`.

```jadpo
Concentration(5mgL)
```

- It is held in kilograms per cubic metre to 9 places.
