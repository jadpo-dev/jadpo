//! Time formats (area 10, "Showing dates and times to people"): what
//! `temporal.format`'s `format:` names, decided once for the checker,
//! which reads them from source, and the runtime, which formats with them.
//!
//! A format is one of the built-in [`BuiltinFormat`]s, `TimeFormat.date_long`,
//! or a declared one, `type ShortDate = TimeFormat { day: ... }`, which lists
//! the [`Component`]s it shows, each with a [`ComponentStyle`]; the locale
//! decides their order and punctuation, as CLDR's skeletons do. Either is
//! written out in source and given to the operation as its
//! [`TimeFormatSpec::written`] text.

use crate::ComponentStyle;

/// One part of a date or a local time a declared format can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Component {
    Weekday,
    Day,
    Month,
    Year,
    Hour,
    Minute,
    Second,
    /// The zone's name, `BST` or `British Summer Time`.
    ZoneName,
}

impl Component {
    /// Every component, in the order a format's text lists them.
    pub const ALL: [Component; 8] = [
        Component::Weekday,
        Component::Day,
        Component::Month,
        Component::Year,
        Component::Hour,
        Component::Minute,
        Component::Second,
        Component::ZoneName,
    ];

    /// The component's name in a `TimeFormat` declaration.
    pub fn name(self) -> &'static str {
        match self {
            Component::Weekday => "weekday",
            Component::Day => "day",
            Component::Month => "month",
            Component::Year => "year",
            Component::Hour => "hour",
            Component::Minute => "minute",
            Component::Second => "second",
            Component::ZoneName => "zone_name",
        }
    }

    pub fn from_name(name: &str) -> Option<Component> {
        Component::ALL
            .into_iter()
            .find(|component| component.name() == name)
    }

    /// The styles it takes, as `Intl.DateTimeFormat` gives them: numbers
    /// as digits, names as words, the month either way.
    pub fn styles(self) -> &'static [ComponentStyle] {
        use ComponentStyle::{Long, Narrow, Numeric, Short, TwoDigit};
        match self {
            Component::Weekday => &[Long, Short, Narrow],
            Component::Month => &[Numeric, TwoDigit, Long, Short, Narrow],
            Component::ZoneName => &[Long, Short],
            Component::Day
            | Component::Year
            | Component::Hour
            | Component::Minute
            | Component::Second => &[Numeric, TwoDigit],
        }
    }

    /// Whether it shows a clock time or a zone, which a `CalendarDate`
    /// doesn't have.
    pub fn needs_time(self) -> bool {
        matches!(
            self,
            Component::Hour | Component::Minute | Component::Second | Component::ZoneName
        )
    }
}

/// What a built-in format shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FormatPart {
    Date,
    Time,
    DateTime,
}

/// How much a built-in format shows, CLDR's four lengths.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FormatLength {
    Full,
    Long,
    Medium,
    Short,
}

/// A built-in format, `TimeFormat.date_time_long`: CLDR's date, time and
/// combined patterns at each length.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BuiltinFormat {
    pub part: FormatPart,
    pub length: FormatLength,
}

impl BuiltinFormat {
    /// Every built-in format, in the order the page lists them.
    pub const ALL: [BuiltinFormat; 12] = {
        use FormatLength::{Full, Long, Medium, Short};
        use FormatPart::{Date, DateTime, Time};
        const fn of(part: FormatPart, length: FormatLength) -> BuiltinFormat {
            BuiltinFormat { part, length }
        }
        [
            of(Date, Full),
            of(Date, Long),
            of(Date, Medium),
            of(Date, Short),
            of(Time, Full),
            of(Time, Long),
            of(Time, Medium),
            of(Time, Short),
            of(DateTime, Full),
            of(DateTime, Long),
            of(DateTime, Medium),
            of(DateTime, Short),
        ]
    };

    /// Its name after `TimeFormat.`: `date_time_long`.
    pub fn name(self) -> &'static str {
        use FormatLength::{Full, Long, Medium, Short};
        use FormatPart::{Date, DateTime, Time};
        match (self.part, self.length) {
            (Date, Full) => "date_full",
            (Date, Long) => "date_long",
            (Date, Medium) => "date_medium",
            (Date, Short) => "date_short",
            (Time, Full) => "time_full",
            (Time, Long) => "time_long",
            (Time, Medium) => "time_medium",
            (Time, Short) => "time_short",
            (DateTime, Full) => "date_time_full",
            (DateTime, Long) => "date_time_long",
            (DateTime, Medium) => "date_time_medium",
            (DateTime, Short) => "date_time_short",
        }
    }

    pub fn from_name(name: &str) -> Option<BuiltinFormat> {
        BuiltinFormat::ALL
            .into_iter()
            .find(|format| format.name() == name)
    }

    /// Whether it shows a clock time, which a `CalendarDate` doesn't have.
    pub fn needs_time(self) -> bool {
        self.part != FormatPart::Date
    }
}

/// A format as `temporal.format` is given it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TimeFormatSpec {
    Builtin(BuiltinFormat),
    /// A declared format's components, each once, in [`Component::ALL`]'s
    /// order.
    Components(Vec<(Component, ComponentStyle)>),
}

impl TimeFormatSpec {
    /// A declared format from its components in any order.
    pub fn components(mut parts: Vec<(Component, ComponentStyle)>) -> TimeFormatSpec {
        parts.sort_by_key(|(component, _)| *component);
        parts.dedup_by_key(|(component, _)| *component);
        TimeFormatSpec::Components(parts)
    }

    /// Whether it shows a clock time or a zone.
    pub fn needs_time(&self) -> bool {
        match self {
            TimeFormatSpec::Builtin(format) => format.needs_time(),
            TimeFormatSpec::Components(parts) => {
                parts.iter().any(|(component, _)| component.needs_time())
            }
        }
    }

    /// The text the operation is given: a built-in format's name,
    /// `date_long`, or each component and its style, `day:numeric
    /// month:short`.
    pub fn written(&self) -> String {
        match self {
            TimeFormatSpec::Builtin(format) => format.name().to_string(),
            TimeFormatSpec::Components(parts) => parts
                .iter()
                .map(|(component, style)| format!("{}:{}", component.name(), style.name()))
                .collect::<Vec<_>>()
                .join(" "),
        }
    }

    /// Reads [`TimeFormatSpec::written`]'s text back; `None` for anything
    /// it never gives.
    pub fn parse(text: &str) -> Option<TimeFormatSpec> {
        if let Some(format) = BuiltinFormat::from_name(text) {
            return Some(TimeFormatSpec::Builtin(format));
        }
        let mut parts = Vec::new();
        for part in text.split(' ') {
            let (component, style) = part.split_once(':')?;
            let component = Component::from_name(component)?;
            let style = ComponentStyle::from_name(style)?;
            if !component.styles().contains(&style) {
                return None;
            }
            parts.push((component, style));
        }
        let spec = TimeFormatSpec::components(parts.clone());
        (spec == TimeFormatSpec::Components(parts)).then_some(spec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_read_back() {
        for format in BuiltinFormat::ALL {
            assert_eq!(BuiltinFormat::from_name(format.name()), Some(format));
        }
        for component in Component::ALL {
            assert_eq!(Component::from_name(component.name()), Some(component));
        }
        assert!(!BuiltinFormat::from_name("date_long").is_some_and(BuiltinFormat::needs_time));
        assert!(BuiltinFormat::from_name("date_time_short").is_some_and(BuiltinFormat::needs_time));
    }

    #[test]
    fn a_format_is_given_as_text_that_reads_back() {
        let short = TimeFormatSpec::components(vec![
            (Component::Year, ComponentStyle::Numeric),
            (Component::Day, ComponentStyle::Numeric),
            (Component::Month, ComponentStyle::Short),
        ]);
        assert_eq!(short.written(), "day:numeric month:short year:numeric");
        assert!(!short.needs_time());
        for spec in [
            short,
            TimeFormatSpec::Builtin(BuiltinFormat::ALL[9]),
            TimeFormatSpec::components(vec![(Component::ZoneName, ComponentStyle::Long)]),
        ] {
            assert_eq!(TimeFormatSpec::parse(&spec.written()), Some(spec));
        }
        for text in ["", "day:long", "month:short day:numeric", "day:numeric day:two_digit", "date_huge"] {
            assert_eq!(TimeFormatSpec::parse(text), None, "{text}");
        }
    }
}
