//! Physical quantities (area 19, "Physical quantities"): each
//! kind holds an exact number in its SI base unit at a fixed scale, so a
//! `Length` is a `Length` whatever unit made it, and `1km == 1000metre`.
//! A unit exists only where a value is made (a suffixed literal,
//! `units.of`, `text.parse`) or shown (`units.in`, `text.format`), and every
//! unit is a row here: its kind, its name in its kind's unit enum
//! (`LengthUnit.kilometre`), its code (the literal suffix, which text and
//! the wire use too), and its size in the base unit as an exact fraction.
//! The products and quotients of two kinds are a table here too, which the
//! checker types and both executors run.

use crate::{Builtin, Rounding};

/// A kind of physical quantity, with `DataSize` and `Bitrate`.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum QuantityKind {
    Length,
    Area,
    Volume,
    Angle,
    Mass,
    Pressure,
    Energy,
    Power,
    Temperature,
    TemperatureDifference,
    Speed,
    Frequency,
    Voltage,
    Current,
    Illuminance,
    SoundLevel,
    Concentration,
    DataSize,
    Bitrate,
}

impl QuantityKind {
    pub const ALL: [QuantityKind; 19] = [
        QuantityKind::Length,
        QuantityKind::Area,
        QuantityKind::Volume,
        QuantityKind::Angle,
        QuantityKind::Mass,
        QuantityKind::Pressure,
        QuantityKind::Energy,
        QuantityKind::Power,
        QuantityKind::Temperature,
        QuantityKind::TemperatureDifference,
        QuantityKind::Speed,
        QuantityKind::Frequency,
        QuantityKind::Voltage,
        QuantityKind::Current,
        QuantityKind::Illuminance,
        QuantityKind::SoundLevel,
        QuantityKind::Concentration,
        QuantityKind::DataSize,
        QuantityKind::Bitrate,
    ];

    /// Its type's name in Jadpo.
    pub const fn name(self) -> &'static str {
        match self {
            QuantityKind::Length => "Length",
            QuantityKind::Area => "Area",
            QuantityKind::Volume => "Volume",
            QuantityKind::Angle => "Angle",
            QuantityKind::Mass => "Mass",
            QuantityKind::Pressure => "Pressure",
            QuantityKind::Energy => "Energy",
            QuantityKind::Power => "Power",
            QuantityKind::Temperature => "Temperature",
            QuantityKind::TemperatureDifference => "TemperatureDifference",
            QuantityKind::Speed => "Speed",
            QuantityKind::Frequency => "Frequency",
            QuantityKind::Voltage => "Voltage",
            QuantityKind::Current => "Current",
            QuantityKind::Illuminance => "Illuminance",
            QuantityKind::SoundLevel => "SoundLevel",
            QuantityKind::Concentration => "Concentration",
            QuantityKind::DataSize => "DataSize",
            QuantityKind::Bitrate => "Bitrate",
        }
    }

    pub fn from_name(name: &str) -> Option<QuantityKind> {
        QuantityKind::ALL
            .into_iter()
            .find(|kind| kind.name() == name)
    }

    /// Its unit enum's name in Jadpo: `LengthUnit`.
    pub const fn unit_enum(self) -> &'static str {
        match self {
            QuantityKind::Length => "LengthUnit",
            QuantityKind::Area => "AreaUnit",
            QuantityKind::Volume => "VolumeUnit",
            QuantityKind::Angle => "AngleUnit",
            QuantityKind::Mass => "MassUnit",
            QuantityKind::Pressure => "PressureUnit",
            QuantityKind::Energy => "EnergyUnit",
            QuantityKind::Power => "PowerUnit",
            QuantityKind::Temperature => "TemperatureUnit",
            QuantityKind::TemperatureDifference => "TemperatureDifferenceUnit",
            QuantityKind::Speed => "SpeedUnit",
            QuantityKind::Frequency => "FrequencyUnit",
            QuantityKind::Voltage => "VoltageUnit",
            QuantityKind::Current => "CurrentUnit",
            QuantityKind::Illuminance => "IlluminanceUnit",
            QuantityKind::SoundLevel => "SoundLevelUnit",
            QuantityKind::Concentration => "ConcentrationUnit",
            QuantityKind::DataSize => "DataSizeUnit",
            QuantityKind::Bitrate => "BitrateUnit",
        }
    }

    /// The kind whose unit enum is called `name`.
    pub fn of_unit_enum(name: &str) -> Option<QuantityKind> {
        QuantityKind::ALL
            .into_iter()
            .find(|kind| kind.unit_enum() == name)
    }

    /// How many places a value keeps in the base unit: its smallest step.
    pub const fn scale(self) -> u32 {
        match self {
            QuantityKind::Length
            | QuantityKind::Area
            | QuantityKind::Speed
            | QuantityKind::Voltage
            | QuantityKind::Current => 6,
            // A microgram is a `Mass`'s smallest step, so a medicine's dose
            // in `mcg` is exact (corrected 2026-10-18).
            QuantityKind::Volume
            | QuantityKind::Angle
            | QuantityKind::Mass
            | QuantityKind::Concentration => 9,
            QuantityKind::Pressure
            | QuantityKind::Energy
            | QuantityKind::Power
            | QuantityKind::Temperature
            | QuantityKind::TemperatureDifference
            | QuantityKind::Frequency
            | QuantityKind::Illuminance => 3,
            QuantityKind::SoundLevel => 2,
            QuantityKind::DataSize | QuantityKind::Bitrate => 0,
        }
    }

    /// Its column's type: `numeric` at its scale.
    pub const fn column_type(self) -> &'static str {
        match self.scale() {
            0 => "numeric(38, 0)",
            2 => "numeric(38, 2)",
            3 => "numeric(38, 3)",
            6 => "numeric(38, 6)",
            _ => "numeric(38, 9)",
        }
    }

    /// Its units, in its enum's order.
    pub fn units(self) -> &'static [Unit] {
        let start = UNITS
            .iter()
            .position(|unit| unit.kind == self)
            .unwrap_or(UNITS.len());
        let count = UNITS.get(start..).map_or(0, |rest| {
            rest.iter().take_while(|unit| unit.kind == self).count()
        });
        UNITS.get(start..start + count).unwrap_or(&[])
    }

    /// The SI base unit its values are held in: what the wire and its
    /// canonical text name.
    pub fn base_unit(self) -> &'static Unit {
        self.unit_named(self.base_name()).unwrap_or(&METRE)
    }

    /// Its SI base unit's name in its enum.
    const fn base_name(self) -> &'static str {
        match self {
            QuantityKind::Length => "metre",
            QuantityKind::Area => "square_metre",
            QuantityKind::Volume => "cubic_metre",
            QuantityKind::Angle => "radian",
            QuantityKind::Mass => "kilogram",
            QuantityKind::Pressure => "pascal",
            QuantityKind::Energy => "joule",
            QuantityKind::Power => "watt",
            QuantityKind::Temperature | QuantityKind::TemperatureDifference => "kelvin",
            QuantityKind::Speed => "metre_per_second",
            QuantityKind::Frequency => "hertz",
            QuantityKind::Voltage => "volt",
            QuantityKind::Current => "ampere",
            QuantityKind::Illuminance => "lux",
            QuantityKind::SoundLevel => "decibel",
            QuantityKind::Concentration => "gram_per_litre",
            QuantityKind::DataSize => "byte",
            QuantityKind::Bitrate => "bit_per_second",
        }
    }

    /// Its unit called `name` in its enum: `kilometre`.
    pub fn unit_named(self, name: &str) -> Option<&'static Unit> {
        self.units().iter().find(|unit| unit.name == name)
    }

    /// Its unit written `code`, exactly as the table spells it: `km`.
    pub fn unit_coded(self, code: &str) -> Option<&'static Unit> {
        self.units().iter().find(|unit| unit.code == code)
    }

    /// Whether `+` and `-` take two of it. A `Temperature` adds only a
    /// `TemperatureDifference` (20 °C and 20 °C isn't 40 °C), and a
    /// `SoundLevel` is logarithmic, so it compares but never adds.
    pub const fn adds_to_itself(self) -> bool {
        !matches!(self, QuantityKind::Temperature | QuantityKind::SoundLevel)
    }

    /// The CLDR category ICU4X has unit names for, where it has them.
    pub const fn cldr_category(self) -> Option<CldrCategory> {
        match self {
            QuantityKind::Length => Some(CldrCategory::Length),
            QuantityKind::Area => Some(CldrCategory::Area),
            QuantityKind::Volume => Some(CldrCategory::Volume),
            QuantityKind::Mass => Some(CldrCategory::Mass),
            _ => None,
        }
    }

    /// The built-in type a value of this kind is.
    pub const fn builtin(self) -> Builtin {
        Builtin::Quantity(self)
    }
}

/// The CLDR categories ICU4X compiles unit names in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CldrCategory {
    Length,
    Area,
    Volume,
    Mass,
}

/// One unit of a kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unit {
    pub kind: QuantityKind,
    /// Its variant in its kind's unit enum: `kilometre`.
    pub name: &'static str,
    /// How it is written after a number: the literal's suffix (`5km`),
    /// and the unit `text.parse`, `text.from` and the wire name. Case
    /// matters, as SI writes units.
    pub code: &'static str,
    /// Whether `code` is a literal's suffix in source. The cubic metre, a
    /// `TemperatureDifference`'s units and the two concentrations chunk
    /// 5.5m2 added (a milligram a decilitre and a millimole of glucose a
    /// litre, whose symbols hold a `/`) have none.
    pub suffix: bool,
    /// Its size in the base unit, `num / den`, exact where the unit is
    /// defined exactly (an inch is 0.0254 m) and to 30 digits for a
    /// degree of angle, π / 180 radians.
    pub num: u128,
    pub den: u128,
    /// Added after scaling, in the base unit, `offset.0 / offset.1`: 0 °C
    /// is 273.15 K.
    pub offset: (i128, u128),
    /// Its symbol for people, where CLDR has no name for it: `°C`.
    pub symbol: &'static str,
    /// Its CLDR unit identifier, in a category ICU4X has names for.
    pub cldr: Option<&'static str>,
    /// What it is, for the editor: "kilometres".
    pub meaning: &'static str,
}

/// A unit with no offset.
const fn unit(
    kind: QuantityKind,
    name: &'static str,
    code: &'static str,
    (num, den): (u128, u128),
    symbol: &'static str,
    cldr: Option<&'static str>,
    meaning: &'static str,
) -> Unit {
    Unit {
        kind,
        name,
        code,
        suffix: true,
        num,
        den,
        offset: (0, 1),
        symbol,
        cldr,
        meaning,
    }
}

/// The same with no literal suffix.
const fn unsuffixed(unit: Unit) -> Unit {
    Unit {
        suffix: false,
        ..unit
    }
}

/// The same with an offset added after scaling.
const fn offset(unit: Unit, offset: (i128, u128)) -> Unit {
    Unit { offset, ..unit }
}

use QuantityKind as K;

/// π / 180, a degree in radians, to 30 significant digits.
const DEGREE: (u128, u128) = (174_532_925_199_432_957_692_369_076_849, 10u128.pow(31));

/// The metre, where a kind's base unit is asked for and none is found,
/// which the table's test rules out.
const METRE: Unit = unit(
    K::Length,
    "metre",
    "metre",
    (1, 1),
    "m",
    Some("meter"),
    "metres",
);

/// Every unit, grouped by kind in [`QuantityKind::ALL`]'s order, each
/// kind's in its enum's order. The suffixes are question 17's table, each
/// checked against the duration suffixes (`ms`, `s`, `m`, `h`, `d`) and
/// each other: `m` is minutes, so metres are `metre`; `in` is a keyword,
/// so inches are `inch`.
pub const UNITS: &[Unit] = &[
    unit(
        K::Length,
        "millimetre",
        "mm",
        (1, 1000),
        "mm",
        Some("millimeter"),
        "millimetres",
    ),
    unit(
        K::Length,
        "centimetre",
        "cm",
        (1, 100),
        "cm",
        Some("centimeter"),
        "centimetres",
    ),
    unit(
        K::Length,
        "metre",
        "metre",
        (1, 1),
        "m",
        Some("meter"),
        "metres",
    ),
    unit(
        K::Length,
        "kilometre",
        "km",
        (1000, 1),
        "km",
        Some("kilometer"),
        "kilometres",
    ),
    unit(
        K::Length,
        "inch",
        "inch",
        (254, 10_000),
        "in",
        Some("inch"),
        "inches",
    ),
    unit(
        K::Length,
        "foot",
        "ft",
        (3048, 10_000),
        "ft",
        Some("foot"),
        "feet",
    ),
    unit(
        K::Length,
        "yard",
        "yd",
        (9144, 10_000),
        "yd",
        Some("yard"),
        "yards",
    ),
    unit(
        K::Length,
        "mile",
        "mi",
        (1_609_344, 1000),
        "mi",
        Some("mile"),
        "miles",
    ),
    unit(
        K::Length,
        "nautical_mile",
        "nmi",
        (1852, 1),
        "nmi",
        Some("nautical-mile"),
        "nautical miles",
    ),
    unit(
        K::Area,
        "square_millimetre",
        "sqmm",
        (1, 1_000_000),
        "mm²",
        None,
        "square millimetres",
    ),
    unit(
        K::Area,
        "square_metre",
        "sqm",
        (1, 1),
        "m²",
        Some("square-meter"),
        "square metres",
    ),
    unit(
        K::Area,
        "square_kilometre",
        "sqkm",
        (1_000_000, 1),
        "km²",
        Some("square-kilometer"),
        "square kilometres",
    ),
    unit(
        K::Area,
        "hectare",
        "ha",
        (10_000, 1),
        "ha",
        Some("hectare"),
        "hectares",
    ),
    unit(
        K::Area,
        "square_foot",
        "sqft",
        (9_290_304, 100_000_000),
        "ft²",
        Some("square-foot"),
        "square feet",
    ),
    unit(
        K::Area,
        "acre",
        "acre",
        (40_468_564_224, 10_000_000),
        "ac",
        Some("acre"),
        "acres",
    ),
    unit(
        K::Volume,
        "millilitre",
        "ml",
        (1, 1_000_000),
        "mL",
        Some("milliliter"),
        "millilitres",
    ),
    unit(
        K::Volume,
        "litre",
        "litre",
        (1, 1000),
        "L",
        Some("liter"),
        "litres",
    ),
    unsuffixed(unit(
        K::Volume,
        "cubic_metre",
        "cubic_metre",
        (1, 1),
        "m³",
        Some("cubic-meter"),
        "cubic metres",
    )),
    unit(
        K::Volume,
        "fluid_ounce",
        "floz",
        (295_735_295_625, 10u128.pow(16)),
        "fl oz",
        Some("fluid-ounce"),
        "US fluid ounces",
    ),
    unit(
        K::Volume,
        "uk_gallon",
        "ukgal",
        (454_609, 100_000_000),
        "gal",
        Some("gallon-imperial"),
        "imperial gallons",
    ),
    unit(
        K::Volume,
        "us_gallon",
        "usgal",
        (3_785_411_784, 1_000_000_000_000),
        "gal",
        Some("gallon"),
        "US gallons",
    ),
    unit(K::Angle, "degree", "deg", DEGREE, "°", None, "degrees"),
    unit(K::Angle, "radian", "rad", (1, 1), "rad", None, "radians"),
    unit(
        K::Mass,
        "microgram",
        "mcg",
        (1, 1_000_000_000),
        "μg",
        Some("microgram"),
        "micrograms",
    ),
    unit(
        K::Mass,
        "milligram",
        "mg",
        (1, 1_000_000),
        "mg",
        Some("milligram"),
        "milligrams",
    ),
    unit(K::Mass, "gram", "g", (1, 1000), "g", Some("gram"), "grams"),
    unit(
        K::Mass,
        "kilogram",
        "kg",
        (1, 1),
        "kg",
        Some("kilogram"),
        "kilograms",
    ),
    unit(
        K::Mass,
        "tonne",
        "t",
        (1000, 1),
        "t",
        Some("tonne"),
        "tonnes",
    ),
    unit(
        K::Mass,
        "ounce",
        "oz",
        (28_349_523_125, 1_000_000_000_000),
        "oz",
        Some("ounce"),
        "ounces",
    ),
    unit(
        K::Mass,
        "pound",
        "lb",
        (45_359_237, 100_000_000),
        "lb",
        Some("pound"),
        "pounds",
    ),
    unit(K::Pressure, "pascal", "Pa", (1, 1), "Pa", None, "pascals"),
    unit(
        K::Pressure,
        "hectopascal",
        "hPa",
        (100, 1),
        "hPa",
        None,
        "hectopascals",
    ),
    unit(
        K::Pressure,
        "kilopascal",
        "kPa",
        (1000, 1),
        "kPa",
        None,
        "kilopascals",
    ),
    unit(K::Pressure, "bar", "bar", (100_000, 1), "bar", None, "bars"),
    unit(
        K::Pressure,
        "millibar",
        "mbar",
        (100, 1),
        "mbar",
        None,
        "millibars",
    ),
    // A pound-force (0.45359237 kg × 9.80665 m/s²) on a square inch.
    unit(
        K::Pressure,
        "psi",
        "psi",
        (44_482_216_152_605, 6_451_600_000),
        "psi",
        None,
        "pounds per square inch",
    ),
    // A millimetre of mercury, as a blood pressure is measured: 133.322387415
    // Pa exactly, as CLDR converts it (chunk 5.5m2), written `120mmHg`,
    // longer than `mm`, so the longest suffix wins.
    unit(
        K::Pressure,
        "millimetre_of_mercury",
        "mmHg",
        (133_322_387_415, 1_000_000_000),
        "mmHg",
        None,
        "millimetres of mercury",
    ),
    unit(K::Energy, "joule", "J", (1, 1), "J", None, "joules"),
    unit(
        K::Energy,
        "kilojoule",
        "kJ",
        (1000, 1),
        "kJ",
        None,
        "kilojoules",
    ),
    unit(
        K::Energy,
        "watt_hour",
        "Wh",
        (3600, 1),
        "Wh",
        None,
        "watt-hours",
    ),
    unit(
        K::Energy,
        "kilowatt_hour",
        "kWh",
        (3_600_000, 1),
        "kWh",
        None,
        "kilowatt-hours",
    ),
    unit(
        K::Energy,
        "kilocalorie",
        "kcal",
        (4184, 1),
        "kcal",
        None,
        "kilocalories",
    ),
    unit(
        K::Power,
        "milliwatt",
        "mW",
        (1, 1000),
        "mW",
        None,
        "milliwatts",
    ),
    unit(K::Power, "watt", "W", (1, 1), "W", None, "watts"),
    unit(
        K::Power,
        "kilowatt",
        "kW",
        (1000, 1),
        "kW",
        None,
        "kilowatts",
    ),
    unit(
        K::Power,
        "megawatt",
        "MW",
        (1_000_000, 1),
        "MW",
        None,
        "megawatts",
    ),
    offset(
        unit(
            K::Temperature,
            "celsius",
            "degC",
            (1, 1),
            "°C",
            None,
            "degrees Celsius",
        ),
        (27_315, 100),
    ),
    offset(
        unit(
            K::Temperature,
            "fahrenheit",
            "degF",
            (5, 9),
            "°F",
            None,
            "degrees Fahrenheit",
        ),
        (229_835, 900),
    ),
    unit(
        K::Temperature,
        "kelvin",
        "kelvin",
        (1, 1),
        "K",
        None,
        "kelvins",
    ),
    unsuffixed(unit(
        K::TemperatureDifference,
        "celsius",
        "degC",
        (1, 1),
        "°C",
        None,
        "degrees Celsius",
    )),
    unsuffixed(unit(
        K::TemperatureDifference,
        "fahrenheit",
        "degF",
        (5, 9),
        "°F",
        None,
        "degrees Fahrenheit",
    )),
    unsuffixed(unit(
        K::TemperatureDifference,
        "kelvin",
        "kelvin",
        (1, 1),
        "K",
        None,
        "kelvins",
    )),
    unit(
        K::Speed,
        "metre_per_second",
        "mps",
        (1, 1),
        "m/s",
        None,
        "metres per second",
    ),
    unit(
        K::Speed,
        "kilometre_per_hour",
        "kmh",
        (5, 18),
        "km/h",
        None,
        "kilometres per hour",
    ),
    unit(
        K::Speed,
        "mile_per_hour",
        "mph",
        (44_704, 100_000),
        "mph",
        None,
        "miles per hour",
    ),
    unit(K::Speed, "knot", "knot", (463, 900), "kn", None, "knots"),
    unit(K::Frequency, "hertz", "Hz", (1, 1), "Hz", None, "hertz"),
    unit(
        K::Frequency,
        "kilohertz",
        "kHz",
        (1000, 1),
        "kHz",
        None,
        "kilohertz",
    ),
    unit(
        K::Frequency,
        "megahertz",
        "MHz",
        (1_000_000, 1),
        "MHz",
        None,
        "megahertz",
    ),
    unit(
        K::Frequency,
        "gigahertz",
        "GHz",
        (1_000_000_000, 1),
        "GHz",
        None,
        "gigahertz",
    ),
    // A beat a minute, as a heart rate is counted: 1/60 Hz (chunk 5.5m2),
    // written `72bpm`.
    unit(
        K::Frequency,
        "beat_per_minute",
        "bpm",
        (1, 60),
        "bpm",
        None,
        "beats per minute",
    ),
    unit(
        K::Voltage,
        "millivolt",
        "mV",
        (1, 1000),
        "mV",
        None,
        "millivolts",
    ),
    unit(K::Voltage, "volt", "V", (1, 1), "V", None, "volts"),
    unit(
        K::Voltage,
        "kilovolt",
        "kV",
        (1000, 1),
        "kV",
        None,
        "kilovolts",
    ),
    unit(
        K::Current,
        "milliampere",
        "mA",
        (1, 1000),
        "mA",
        None,
        "milliamperes",
    ),
    unit(K::Current, "ampere", "A", (1, 1), "A", None, "amperes"),
    unit(K::Illuminance, "lux", "lx", (1, 1), "lx", None, "lux"),
    unit(
        K::SoundLevel,
        "decibel",
        "dB",
        (1, 1),
        "dB",
        None,
        "decibels",
    ),
    unit(
        K::Concentration,
        "milligram_per_litre",
        "mgL",
        (1, 1000),
        "mg/L",
        None,
        "milligrams per litre",
    ),
    unit(
        K::Concentration,
        "gram_per_litre",
        "gL",
        (1, 1),
        "g/L",
        None,
        "grams per litre",
    ),
    // A milligram a decilitre, as a US laboratory reports glucose: 0.01 g/L
    // (chunk 5.5m2), read by its code, `mgdL`, with no literal suffix.
    unsuffixed(unit(
        K::Concentration,
        "milligram_per_decilitre",
        "mgdL",
        (1, 100),
        "mg/dL",
        None,
        "milligrams per decilitre",
    )),
    // A millimole of glucose a litre, as a UK or European meter reports
    // blood glucose: glucose's molar mass, 180.1557 g/mol as CLDR converts
    // its `milligram-ofglucose-per-deciliter`, makes it 0.1801557 g/L
    // (chunk 5.5m2). It is glucose's alone, so its code is its name.
    unsuffixed(unit(
        K::Concentration,
        "millimole_of_glucose_per_litre",
        "millimole_of_glucose_per_litre",
        (1_801_557, 10_000_000),
        "mmol/L",
        None,
        "millimoles of glucose per litre",
    )),
    unit(K::DataSize, "byte", "B", (1, 1), "B", None, "bytes"),
    unit(
        K::DataSize,
        "kilobyte",
        "kB",
        (1000, 1),
        "kB",
        None,
        "kilobytes",
    ),
    unit(
        K::DataSize,
        "megabyte",
        "MB",
        (1_000_000, 1),
        "MB",
        None,
        "megabytes",
    ),
    unit(
        K::DataSize,
        "gigabyte",
        "GB",
        (1_000_000_000, 1),
        "GB",
        None,
        "gigabytes",
    ),
    unit(
        K::DataSize,
        "terabyte",
        "TB",
        (1_000_000_000_000, 1),
        "TB",
        None,
        "terabytes",
    ),
    unit(
        K::DataSize,
        "kibibyte",
        "KiB",
        (1024, 1),
        "KiB",
        None,
        "kibibytes",
    ),
    unit(
        K::DataSize,
        "mebibyte",
        "MiB",
        (1_048_576, 1),
        "MiB",
        None,
        "mebibytes",
    ),
    unit(
        K::DataSize,
        "gibibyte",
        "GiB",
        (1_073_741_824, 1),
        "GiB",
        None,
        "gibibytes",
    ),
    unit(
        K::DataSize,
        "tebibyte",
        "TiB",
        (1_099_511_627_776, 1),
        "TiB",
        None,
        "tebibytes",
    ),
    unit(
        K::Bitrate,
        "bit_per_second",
        "bps",
        (1, 1),
        "bit/s",
        None,
        "bits per second",
    ),
    unit(
        K::Bitrate,
        "kilobit_per_second",
        "kbps",
        (1000, 1),
        "kbit/s",
        None,
        "kilobits per second",
    ),
    unit(
        K::Bitrate,
        "megabit_per_second",
        "Mbps",
        (1_000_000, 1),
        "Mbit/s",
        None,
        "megabits per second",
    ),
    unit(
        K::Bitrate,
        "gigabit_per_second",
        "Gbps",
        (1_000_000_000, 1),
        "Gbit/s",
        None,
        "gigabits per second",
    ),
];

/// The unit a literal's suffix names, matched exactly.
pub fn unit_of_suffix(suffix: &str) -> Option<&'static Unit> {
    UNITS.iter().find(|unit| unit.suffix && unit.code == suffix)
}

/// The suffixes `suffix` misses only by case, `mW` and `MW` for `mw`
/// (question 17: a near miss by case is an error naming each).
pub fn near_misses(suffix: &str) -> Vec<&'static Unit> {
    UNITS
        .iter()
        .filter(|unit| unit.suffix && unit.code.eq_ignore_ascii_case(suffix) && unit.code != suffix)
        .collect()
}

/// Why a suffixed number isn't a quantity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiteralProblem {
    /// No unit has this suffix, even ignoring case.
    Unknown(String),
    /// Units whose suffixes it misses only by case.
    NearMiss(String, Vec<&'static Unit>),
    /// The value doesn't fit once in the base unit.
    Overflow,
}

/// A suffixed literal as written, `2.5kg` or `-40degC`: its unit and its
/// value in the base unit, at its kind's scale, rounded half to even.
pub fn parse_literal(text: &str) -> Result<(&'static Unit, i128), LiteralProblem> {
    let (negative, rest) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let at = rest
        .find(|c: char| c.is_ascii_alphabetic())
        .ok_or_else(|| LiteralProblem::Unknown(String::new()))?;
    let (number, suffix) = rest.split_at(at);
    let unit = match unit_of_suffix(suffix) {
        Some(unit) => unit,
        None => {
            let near = near_misses(suffix);
            return Err(if near.is_empty() {
                LiteralProblem::Unknown(suffix.to_string())
            } else {
                LiteralProblem::NearMiss(suffix.to_string(), near)
            });
        }
    };
    let num = crate::Num::parse(number).ok_or(LiteralProblem::Overflow)?;
    let mantissa = if negative {
        -num.mantissa
    } else {
        num.mantissa
    };
    let base = to_base(mantissa, num.scale, unit).ok_or(LiteralProblem::Overflow)?;
    Ok((unit, base))
}

/// The suffix a literal is written with, after its number: `km` in `5km`.
pub fn suffix_of(text: &str) -> &str {
    let digits = text.trim_start_matches('-');
    let at = digits
        .find(|c: char| c.is_ascii_alphabetic())
        .unwrap_or(digits.len());
    digits.get(at..).unwrap_or("")
}

// --- exact arithmetic -------------------------------------------------------

/// `mantissa × 10^-scale` of `unit`, in the base unit at its kind's scale,
/// rounded half to even; `None` when it doesn't fit 38 digits.
pub fn to_base(mantissa: i128, scale: u32, unit: &Unit) -> Option<i128> {
    let target = unit.kind.scale();
    if unit.offset.0 == 0 {
        return ratio(
            mantissa,
            &[unit.num],
            target,
            &[unit.den],
            scale,
            Rounding::HalfEven,
        );
    }
    // (m × num × odd + on × den × 10^scale) × 10^target
    //   / (10^scale × den × odd)
    let (on, od) = unit.offset;
    let num = i128::try_from(unit.num).ok()?;
    let den = i128::try_from(unit.den).ok()?;
    let od = i128::try_from(od).ok()?;
    let multiplied = mantissa.checked_mul(num)?.checked_mul(od)?;
    let shifted = on
        .checked_mul(den)?
        .checked_mul(10i128.checked_pow(scale)?)?;
    let sum = multiplied.checked_add(shifted)?;
    ratio(
        sum,
        &[1],
        target,
        &[u128::try_from(den.checked_mul(od)?).ok()?],
        scale,
        Rounding::HalfEven,
    )
}

/// A value held in the base unit at its kind's scale, in `unit`, at
/// `places` places rounded as `rounding` says; `None` past 38 digits.
pub fn from_base(amount: i128, unit: &Unit, places: u32, rounding: Rounding) -> Option<i128> {
    let scale = unit.kind.scale();
    if unit.offset.0 == 0 {
        return ratio(amount, &[unit.den], places, &[unit.num], scale, rounding);
    }
    // (amount × odd - on × 10^scale) × den / (10^scale × odd × num)
    let (on, od) = unit.offset;
    let od = i128::try_from(od).ok()?;
    let unshifted = amount
        .checked_mul(od)?
        .checked_sub(on.checked_mul(10i128.checked_pow(scale)?)?)?;
    ratio(
        unshifted,
        &[unit.den],
        places,
        &[unit.num, u128::try_from(od).ok()?],
        scale,
        rounding,
    )
}

/// A value held in the base unit as people see it in `unit`: exact where
/// the conversion ends within the places the kind's scale gives in that
/// unit (2.4762 m is 247.62 cm), and otherwise rounded half to even to one
/// place fewer, so a value made in the unit reads back as written: `1oz`
/// is held as 0.028349523 kg, which is 0.99999998 oz, shown as 1. The
/// mantissa and its places, with no trailing zeros.
pub fn shown_in(amount: i128, unit: &Unit) -> Option<(i128, u32)> {
    let scale = unit.kind.scale();
    let places =
        u32::try_from((i64::from(scale) + i64::from(magnitude(unit.num, unit.den))).max(0)).ok()?;
    let exact = (0..=places).find_map(|places| {
        let down = from_base(amount, unit, places, Rounding::Down)?;
        let up = from_base(amount, unit, places, Rounding::Up)?;
        (down == up).then_some((down, places))
    });
    let (mut mantissa, mut places) = match exact {
        Some(found) => found,
        None => {
            let places = places.saturating_sub(1);
            (from_base(amount, unit, places, Rounding::HalfEven)?, places)
        }
    };
    while places > 0 && mantissa % 10 == 0 {
        mantissa /= 10;
        places -= 1;
    }
    Some((mantissa, places))
}

/// The power of ten `num / den` is in: the largest `k` with `den × 10^k`
/// no more than `num`, so 1609.344 is 3 and 0.0254 is -2.
fn magnitude(num: u128, den: u128) -> i32 {
    let mut k = 0i32;
    let (mut num, mut den) = (Wide::from(num), Wide::from(den));
    if num.less_than(&den) {
        while num.less_than(&den) {
            num = num.times(10);
            k -= 1;
        }
        return k;
    }
    loop {
        let next = den.times(10);
        if num.less_than(&next) {
            return k;
        }
        den = next;
        k += 1;
    }
}

/// `value × Π multipliers × 10^up / (Π divisors × 10^down)`, rounded as
/// `rounding` says, worked out as wide as it needs; `None` when a divisor
/// is zero or the result is past 38 digits.
pub fn ratio(
    value: i128,
    multipliers: &[u128],
    up: u32,
    divisors: &[u128],
    down: u32,
    rounding: Rounding,
) -> Option<i128> {
    let (up, down) = if up >= down {
        (up - down, 0)
    } else {
        (0, down - up)
    };
    let mut numerator = Wide::from(value.unsigned_abs());
    for factor in multipliers {
        numerator = numerator.times(*factor);
    }
    for _ in 0..up {
        numerator = numerator.times(10);
    }
    let mut divisor = Wide::from(1);
    for factor in divisors {
        divisor = divisor.times(*factor);
    }
    for _ in 0..down {
        divisor = divisor.times(10);
    }
    let divisor = divisor.to_u128()?;
    if divisor == 0 {
        return None;
    }
    let (mut quotient, remainder) = numerator.div_rem(divisor);
    if remainder != 0 {
        let twice = remainder << 1;
        let step = match rounding {
            Rounding::Up => true,
            Rounding::Down => false,
            Rounding::HalfUp => twice >= divisor,
            Rounding::HalfEven => twice > divisor || (twice == divisor && quotient.is_odd()),
        };
        if step {
            quotient = quotient.plus_one();
        }
    }
    let magnitude = quotient.to_u128()?;
    if magnitude >= 10u128.pow(38) {
        return None;
    }
    let magnitude = i128::try_from(magnitude).ok()?;
    Some(if value < 0 { -magnitude } else { magnitude })
}

/// An unsigned whole number as wide as a product needs: 64-bit limbs,
/// least significant first.
#[derive(Clone, Debug)]
struct Wide(Vec<u64>);

impl Wide {
    fn from(n: u128) -> Wide {
        Wide(vec![n as u64, (n >> 64) as u64])
    }

    fn times(&self, factor: u128) -> Wide {
        let other = [factor as u64, (factor >> 64) as u64];
        let mut out = vec![0u64; self.0.len() + 2];
        for (i, &a) in self.0.iter().enumerate() {
            let mut carry = 0u128;
            for (j, &b) in other.iter().enumerate() {
                let Some(slot) = out.get_mut(i + j) else {
                    continue;
                };
                let wide = u128::from(a) * u128::from(b) + u128::from(*slot) + carry;
                *slot = wide as u64;
                carry = wide >> 64;
            }
            let mut k = i + other.len();
            while carry != 0 {
                let Some(slot) = out.get_mut(k) else { break };
                let wide = u128::from(*slot) + carry;
                *slot = wide as u64;
                carry = wide >> 64;
                k += 1;
            }
        }
        while out.len() > 2 && out.last() == Some(&0) {
            out.pop();
        }
        Wide(out)
    }

    /// Whether it is below `other`.
    fn less_than(&self, other: &Wide) -> bool {
        let limbs = self.0.len().max(other.0.len());
        for at in (0..limbs).rev() {
            let (a, b) = (
                self.0.get(at).copied().unwrap_or(0),
                other.0.get(at).copied().unwrap_or(0),
            );
            if a != b {
                return a < b;
            }
        }
        false
    }

    fn is_odd(&self) -> bool {
        self.0.first().is_some_and(|limb| limb & 1 == 1)
    }

    fn plus_one(mut self) -> Wide {
        for limb in &mut self.0 {
            let (sum, carried) = limb.overflowing_add(1);
            *limb = sum;
            if !carried {
                return self;
            }
        }
        self.0.push(1);
        self
    }

    fn div_rem(&self, divisor: u128) -> (Wide, u128) {
        let mut quotient = vec![0u64; self.0.len()];
        let mut remainder = 0u128;
        for (limb, out) in self.0.iter().zip(quotient.iter_mut()).rev() {
            for bit in (0..64).rev() {
                let carried = remainder >> 127 == 1;
                remainder = (remainder << 1) | u128::from((limb >> bit) & 1);
                if carried || remainder >= divisor {
                    remainder = remainder.wrapping_sub(divisor);
                    *out |= 1 << bit;
                }
            }
        }
        (Wide(quotient), remainder)
    }

    fn to_u128(&self) -> Option<u128> {
        if self.0.iter().skip(2).any(|&limb| limb != 0) {
            return None;
        }
        let low = u128::from(self.0.first().copied().unwrap_or(0));
        let high = u128::from(self.0.get(1).copied().unwrap_or(0));
        Some(low | (high << 64))
    }
}

// --- the product and quotient table ----------------------------------------

/// One side of a row of the table: a quantity, or a `Duration`, which
/// takes part without becoming a quantity (area 10's type).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Quantity(QuantityKind),
    Duration,
}

/// The products and quotients of two kinds (area 19, "Physical
/// quantities"): left, operator, right, and what they give. `+` and `-`
/// of one kind, and `*` and `/` by a plain number, aren't rows; nor is a
/// kind divided by itself, which gives a plain `Decimal`.
pub const PRODUCTS: &[(Side, char, Side, Side)] = &[
    (q(K::Length), '*', q(K::Length), q(K::Area)),
    (q(K::Area), '*', q(K::Length), q(K::Volume)),
    (q(K::Area), '/', q(K::Length), q(K::Length)),
    (q(K::Volume), '/', q(K::Area), q(K::Length)),
    (q(K::Volume), '/', q(K::Length), q(K::Area)),
    (q(K::Length), '/', Side::Duration, q(K::Speed)),
    (q(K::Speed), '*', Side::Duration, q(K::Length)),
    (q(K::Length), '/', q(K::Speed), Side::Duration),
    (q(K::Energy), '/', Side::Duration, q(K::Power)),
    (q(K::Power), '*', Side::Duration, q(K::Energy)),
    (q(K::Voltage), '*', q(K::Current), q(K::Power)),
    (q(K::Power), '/', q(K::Voltage), q(K::Current)),
    (q(K::Power), '/', q(K::Current), q(K::Voltage)),
    (q(K::Mass), '/', q(K::Volume), q(K::Concentration)),
    (q(K::Concentration), '*', q(K::Volume), q(K::Mass)),
    (q(K::DataSize), '/', Side::Duration, q(K::Bitrate)),
    (q(K::Bitrate), '*', Side::Duration, q(K::DataSize)),
    (
        q(K::Temperature),
        '-',
        q(K::Temperature),
        q(K::TemperatureDifference),
    ),
    (
        q(K::Temperature),
        '+',
        q(K::TemperatureDifference),
        q(K::Temperature),
    ),
    (
        q(K::Temperature),
        '-',
        q(K::TemperatureDifference),
        q(K::Temperature),
    ),
];

const fn q(kind: QuantityKind) -> Side {
    Side::Quantity(kind)
}

/// What `left op right` gives by the table, where it is a row.
pub fn product(left: Side, op: char, right: Side) -> Option<Side> {
    PRODUCTS
        .iter()
        .find(|(l, o, r, _)| *l == left && *o == op && *r == right)
        .map(|(_, _, _, gives)| *gives)
}

impl Side {
    /// Its scale: a duration is held in milliseconds, seconds at 3 places.
    pub const fn scale(self) -> u32 {
        match self {
            Side::Quantity(kind) => kind.scale(),
            Side::Duration => 3,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Side::Quantity(kind) => kind.name(),
            Side::Duration => "Duration",
        }
    }
}

/// `left op right` by the table, each a mantissa at its side's scale in
/// its SI base unit (a duration in milliseconds): the result's mantissa at
/// its side's scale, exact and then rounded half to even. A byte is 8
/// bits, so a `DataSize` over a `Duration` is bits per second. `None` for
/// no row, a zero divisor, or past 38 digits.
pub fn apply(left: (Side, i128), op: char, right: (Side, i128)) -> Option<(Side, i128)> {
    let gives = product(left.0, op, right.0)?;
    let (a, b) = (left.1, right.1);
    let (sa, sb, sg) = (left.0.scale(), right.0.scale(), gives.scale());
    // Bits for a byte, either way.
    let (bits_up, bits_down) = match (left.0, gives) {
        (Side::Quantity(K::DataSize), _) => (8, 1),
        (Side::Quantity(K::Bitrate), Side::Quantity(K::DataSize)) => (1, 8),
        _ => (1, 1),
    };
    let mantissa = match op {
        '*' => {
            let negative = (a < 0) != (b < 0);
            let size = ratio(
                a.abs(),
                &[b.unsigned_abs(), bits_up],
                sg,
                &[bits_down],
                sa + sb,
                Rounding::HalfEven,
            )?;
            if negative { -size } else { size }
        }
        '/' => {
            if b == 0 {
                return None;
            }
            let negative = (a < 0) != (b < 0);
            let size = ratio(
                a.abs(),
                &[bits_up],
                sg + sb,
                &[b.unsigned_abs(), bits_down],
                sa,
                Rounding::HalfEven,
            )?;
            if negative { -size } else { size }
        }
        // Temperatures in kelvin at one scale: plain addition.
        '+' => a.checked_add(b)?,
        '-' => a.checked_sub(b)?,
        _ => return None,
    };
    Some((gives, mantissa))
}

// --- text ------------------------------------------------------------------

/// Why text isn't a quantity of a kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextProblem {
    /// No number, or not a number and a unit at all.
    Malformed,
    /// A unit, but not one of the kind's.
    UnknownUnit,
}

/// Text as `text.parse` reads a quantity of `kind`: a number, an optional
/// space, and one of the kind's units by its code (`5 km`, `2.5kg`), in
/// the base unit at the kind's scale, rounded half to even.
pub fn parse_text(kind: QuantityKind, text: &str) -> Result<i128, TextProblem> {
    let text = text.trim();
    let at = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
        .unwrap_or(text.len());
    let (number, rest) = text.split_at(at);
    let code = rest.trim_start();
    let number = number.strip_prefix('+').unwrap_or(number);
    let num = crate::Num::parse(number)
        .filter(|_| {
            !number.is_empty()
                && !number.starts_with('.')
                && !number.ends_with('.')
                && number
                    .trim_start_matches('-')
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '.')
        })
        .ok_or(TextProblem::Malformed)?;
    if code.is_empty() || code.contains(char::is_whitespace) {
        return Err(TextProblem::Malformed);
    }
    let unit = kind.unit_coded(code).ok_or(TextProblem::UnknownUnit)?;
    to_base(num.mantissa, num.scale, unit).ok_or(TextProblem::Malformed)
}

/// A value in its base unit as a number with no trailing zeros: `2.5` for
/// 2.500000 kg.
pub fn number_text(amount: i128, scale: u32) -> String {
    let mut mantissa = amount;
    let mut places = scale;
    while places > 0 && mantissa % 10 == 0 {
        mantissa /= 10;
        places -= 1;
    }
    crate::Num {
        mantissa,
        scale: places,
    }
    .to_string()
}

/// A quantity's canonical text: its number in the base unit with no
/// trailing zeros, a space and the base unit's code, `2.5 kg`, `1000
/// metre`.
pub fn canonical(kind: QuantityKind, amount: i128) -> String {
    format!(
        "{} {}",
        number_text(amount, kind.scale()),
        kind.base_unit().code
    )
}

/// How a value is written in code, `2.5kg`, as reports and failed
/// assertions show it: its base unit's suffix, or `units.of` for the
/// kinds whose base unit has none.
pub fn written(kind: QuantityKind, amount: i128) -> String {
    let base = kind.base_unit();
    let number = number_text(amount, kind.scale());
    if base.suffix {
        format!("{number}{}", base.code)
    } else {
        format!("units.of({number}, {}.{})", kind.unit_enum(), base.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(text: &str) -> i128 {
        parse_literal(text)
            .map(|(_, value)| value)
            .unwrap_or_else(|problem| panic!("{text}: {problem:?}"))
    }

    #[test]
    fn every_kind_has_its_units_in_order_and_one_base() {
        assert_eq!(QuantityKind::ALL.len(), 19);
        for kind in QuantityKind::ALL {
            let units = kind.units();
            assert!(!units.is_empty(), "{}", kind.name());
            assert_eq!(QuantityKind::from_name(kind.name()), Some(kind));
            assert_eq!(QuantityKind::of_unit_enum(kind.unit_enum()), Some(kind));
            let base = kind.base_unit();
            assert_eq!(base.kind, kind, "{} has its base unit", kind.name());
            assert_eq!(
                (base.num, base.den, base.offset.0),
                (1, 1, 0),
                "{}",
                kind.name()
            );
        }
        let grouped: usize = QuantityKind::ALL
            .iter()
            .map(|kind| kind.units().len())
            .sum();
        assert_eq!(grouped, UNITS.len(), "units are grouped by kind");
    }

    #[test]
    fn suffixes_clash_with_nothing() {
        let durations = ["ms", "s", "m", "h", "d"];
        let mut seen = std::collections::BTreeSet::new();
        for unit in UNITS.iter().filter(|unit| unit.suffix) {
            assert!(!durations.contains(&unit.code), "{}", unit.code);
            assert!(seen.insert(unit.code), "{} once", unit.code);
            assert!(
                unit.code.chars().all(|c| c.is_ascii_alphabetic()),
                "{}",
                unit.code
            );
        }
        for refused in ["m", "in", "l", "gal", "cal", "b", "K"] {
            assert!(unit_of_suffix(refused).is_none(), "{refused}");
        }
    }

    #[test]
    fn literals_hold_the_base_unit() {
        assert_eq!(base("1km"), base("1000metre"));
        assert_eq!(base("1km"), 1_000_000_000);
        assert_eq!(base("3inch"), 76_200);
        assert_eq!(base("45cm"), 450_000);
        assert_eq!(base("2.5kg"), 2_500_000_000);
        assert_eq!(base("250g"), 250_000_000);
        // A microgram is exact, as a medicine's dose needs, and so is a
        // pound (corrected 2026-10-18).
        assert_eq!(base("1mcg"), 1);
        assert_eq!(base("1lb"), 453_592_370);
        assert_eq!(base("0degC"), 273_150);
        assert_eq!(base("-40degC"), base("-40degF"));
        assert_eq!(base("212degF"), 373_150);
        assert_eq!(base("1KiB"), 1024);
        assert_eq!(base("90deg"), 1_570_796_327);
        assert_eq!(base("100kmh"), 27_777_778);
        assert_eq!(base("1psi"), 6_894_757);
        assert_eq!(
            parse_literal("5mw").map(|(unit, _)| unit.code),
            Err(LiteralProblem::NearMiss(
                "mw".into(),
                vec![
                    QuantityKind::Power.unit_coded("mW").unwrap(),
                    QuantityKind::Power.unit_coded("MW").unwrap()
                ]
            ))
        );
        assert!(matches!(
            parse_literal("5zz"),
            Err(LiteralProblem::Unknown(_))
        ));
    }

    #[test]
    fn clinical_units_read_back_as_written() {
        let unit = |kind: QuantityKind, name: &str| kind.unit_named(name).unwrap();
        let held = |kind: QuantityKind, name: &str, n: i128, scale: u32| {
            to_base(n, scale, unit(kind, name)).unwrap()
        };
        // 120 mmHg is 15,998.686 Pa to the kind's 3 places, shown as 120.
        let mmhg = unit(QuantityKind::Pressure, "millimetre_of_mercury");
        let systolic = held(QuantityKind::Pressure, "millimetre_of_mercury", 120, 0);
        assert_eq!(systolic, 15_998_686);
        assert_eq!(shown_in(systolic, mmhg), Some((120, 0)));
        // 72 bpm is 1.2 Hz exactly; 73 bpm is 1.217 Hz, still shown as 73.
        let bpm = unit(QuantityKind::Frequency, "beat_per_minute");
        assert_eq!(
            held(QuantityKind::Frequency, "beat_per_minute", 72, 0),
            1200
        );
        let pulse = held(QuantityKind::Frequency, "beat_per_minute", 73, 0);
        assert_eq!(pulse, 1217);
        assert_eq!(shown_in(pulse, bpm), Some((73, 0)));
        // 5.5 mmol/L of glucose is 0.99085635 g/L exactly, and 99 mg/dL
        // 0.99 g/L; each reads back in its own unit.
        let glucose = unit(
            QuantityKind::Concentration,
            "millimole_of_glucose_per_litre",
        );
        let mgdl = unit(QuantityKind::Concentration, "milligram_per_decilitre");
        let fasting = held(
            QuantityKind::Concentration,
            "millimole_of_glucose_per_litre",
            55,
            1,
        );
        assert_eq!(fasting, 990_856_350);
        assert_eq!(shown_in(fasting, glucose), Some((55, 1)));
        let us = held(
            QuantityKind::Concentration,
            "milligram_per_decilitre",
            99,
            0,
        );
        assert_eq!(us, 990_000_000);
        assert_eq!(shown_in(us, mgdl), Some((99, 0)));
        assert_eq!(parse_text(QuantityKind::Pressure, "120 mmHg"), Ok(systolic));
        // `mmHg` and `bpm` are literals' suffixes (decided by the lead
        // session, 2026-10-18), `mmHg` beside `mm` by the longest suffix;
        // the concentrations, whose symbols hold a `/`, aren't.
        assert_eq!(base("120mmHg"), systolic);
        assert_eq!(base("120mm"), 120_000);
        assert_eq!(base("72bpm"), 1200);
        assert!(unit_of_suffix("mgdL").is_none());
        assert!(matches!(
            parse_literal("120mmhg"),
            Err(LiteralProblem::NearMiss(..))
        ));
    }

    #[test]
    fn values_come_back_in_any_unit() {
        let total = base("2metre") + base("45cm") + base("3inch");
        assert_eq!(total, 2_526_200);
        let centimetre = QuantityKind::Length.unit_named("centimetre").unwrap();
        assert_eq!(shown_in(total, centimetre), Some((25262, 2)));
        let inch = QuantityKind::Length.unit_named("inch").unwrap();
        assert_eq!(shown_in(base("3inch"), inch), Some((3, 0)));
        let celsius = QuantityKind::Temperature.unit_named("celsius").unwrap();
        assert_eq!(
            from_base(base("212degF"), celsius, 1, Rounding::HalfEven),
            Some(1000)
        );
        let degree = QuantityKind::Angle.unit_named("degree").unwrap();
        assert_eq!(
            from_base(base("90deg"), degree, 6, Rounding::HalfEven),
            Some(90_000_000)
        );
    }

    #[test]
    fn the_table_multiplies_and_divides_exactly() {
        let length = Side::Quantity(K::Length);
        let area = apply((length, base("3metre")), '*', (length, base("2metre")));
        assert_eq!(area, Some((Side::Quantity(K::Area), 6_000_000)));
        let speed = apply((length, base("100metre")), '/', (Side::Duration, 8_000));
        assert_eq!(speed, Some((Side::Quantity(K::Speed), 12_500_000)));
        let rate = apply(
            (Side::Quantity(K::DataSize), base("1MB")),
            '/',
            (Side::Duration, 1_000),
        );
        assert_eq!(rate, Some((Side::Quantity(K::Bitrate), 8_000_000)));
        let size = apply(
            (Side::Quantity(K::Bitrate), 8_000_000),
            '*',
            (Side::Duration, 1_000),
        );
        assert_eq!(size, Some((Side::Quantity(K::DataSize), 1_000_000)));
        let warmer = apply(
            (Side::Quantity(K::Temperature), base("25degC")),
            '-',
            (Side::Quantity(K::Temperature), base("20degC")),
        );
        assert_eq!(
            warmer,
            Some((Side::Quantity(K::TemperatureDifference), 5_000))
        );
        assert_eq!(product(length, '*', Side::Quantity(K::Mass)), None);
    }

    #[test]
    fn text_is_read_with_the_kind_s_units() {
        assert_eq!(parse_text(K::Length, "5 km"), Ok(5_000_000_000));
        assert_eq!(parse_text(K::Length, "5km"), Ok(5_000_000_000));
        assert_eq!(parse_text(K::Length, "5 kg"), Err(TextProblem::UnknownUnit));
        assert_eq!(parse_text(K::Length, "km"), Err(TextProblem::Malformed));
        assert_eq!(parse_text(K::Length, "5"), Err(TextProblem::Malformed));
        assert_eq!(canonical(K::Mass, 2_500_000_000), "2.5 kg");
        assert_eq!(canonical(K::Mass, 1), "0.000000001 kg");
        let microgram = K::Mass.unit_named("microgram").unwrap();
        assert_eq!(shown_in(1, microgram), Some((1, 0)));
        assert_eq!(canonical(K::Length, 1_000_000_000), "1000 metre");
        assert_eq!(
            written(K::Volume, 1_000_000),
            "units.of(0.001, VolumeUnit.cubic_metre)"
        );
    }
}
