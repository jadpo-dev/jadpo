# BloodType

A person's blood group: `O-`, `AB+` and so on.

Use it in medical records, or wherever else someone's blood group is recorded.

```jadpo
BloodType.o_negative
```

# Icd10Code

A diagnosis's ICD-10 code, with its dot: `I21.9`.

Use it to record a diagnosis in a form health systems share. For clinical concepts
beyond diagnoses, use `SnomedConcept`.

```jadpo
Icd10Code("I21.9")
```

- Write it in capitals with its dot. `I219` is refused.

# SnomedConcept

A SNOMED CT concept's number, the clinical term a health system records: `22298006`.

Use it for clinical terms such as procedures and findings. For a diagnosis in reporting,
use `Icd10Code`.

```jadpo
SnomedConcept("22298006")
```

- It checks its own last digit, so a mistyped number is usually refused.

# Dose

An amount of a medicine: `500 mg` or `5 ml`.

Use it for a single dose. For how and how often to take one, use `DosageInstruction`.

```jadpo
Dose.mass { amount: 500mg }
```

- The other variant is `Dose.volume { amount: ... }`.
- It must be above zero.

# DosageInstruction

How to take a medicine: a dose, a route and how often.

Use it for a prescription's directions, shown as `500 mg, Oral use, 2 times per 1 day`.
For the amount alone, use `Dose`.

```jadpo
DosageInstruction {
    dose: Dose.mass { amount: 500mg }
    route: RouteOfAdministration.oral
    times: 2
    per: 1d
    as_needed: false
}
```

- `times` must be at least 1, and `per` above nothing.

# RouteOfAdministration

How a medicine is taken: `oral`, `intravenous` and so on.

Use it as part of a `DosageInstruction`, or to filter medicines by route.

```jadpo
RouteOfAdministration.oral
```

- `text.format` shows the standard wording, such as "Oral use".

# HeartRate

A heart rate: `72bpm`.

Use it for a person's pulse, from 0 to 300 beats a minute. For any other rate, use
`Frequency`.

```jadpo
HeartRate(72bpm)
```

- It is a `Frequency`, so in JSON and storage it is in hertz: 72 bpm is 1.2 Hz.

# BloodPressure

A blood pressure reading: `120/80 mmHg`.

Use it for a person's reading. For any other pressure, use `Pressure`.

```jadpo
BloodPressure {
    systolic: 120mmHg
    diastolic: 80mmHg
}
```

- `diastolic` must be below `systolic`.

# BloodGlucose

A blood sugar level, shown `5.5 mmol/L`.

Use it for a person's reading. For any other concentration, use `Concentration`.

```jadpo
BloodGlucose(0.99gL)
```

- It is shown in millimoles per litre, so `0.99gL` shows as `5.5 mmol/L`. To start
  from that figure, use `units.of(5.5, ConcentrationUnit.millimole_of_glucose_per_litre)`.
- It can't be below zero.

# Allergen

One of the 14 allergens a UK or EU food label must name: `milk`, `celery` and so on.

Use it to mark what's in a dish or a product. For a person's own allergy to something
else, use your own type, since this list holds only those 14.

```jadpo
Allergen.milk
```

- `Allergen.cereals_containing_gluten` is the name for gluten. `text.format` shows the
  UK Food Standards Agency's wording.
