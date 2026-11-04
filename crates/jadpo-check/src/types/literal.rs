//! A literal takes the type it goes into (area 2, question 15): where the
//! expected type is known, a literal must be the right kind of value for
//! it and follow every rule on it and the types it builds on, checked while
//! compiling.

use jadpo_diag::{Diagnostic, Span, rule};

use super::rules::{Broken, Const, check_builtin, check_rules};
use super::shape::FieldsBase;
use super::table::{Def, FieldDef, TypeView, standard_fields};
use super::ty::{Prim, Ty};

/// Why a literal can't be the expected type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// The wrong kind of value, with a replacement when one is obvious.
    Kind { fix: Option<String> },
    /// The kind is right, but the built-in type refuses it.
    Builtin(String),
    /// A rule is broken: which type declares it, and how.
    Broken(String, Broken),
    /// A bare number where a quantity of this kind goes, which needs its
    /// unit (area 19, `type.unit_required`).
    UnitRequired(jadpo_std::QuantityKind),
    /// A bare number where a percentage goes, which could be a ratio or a
    /// percentage already (area 19): written with `%` instead.
    PercentRequired(jadpo_std::Num),
    /// Text where a standard object type with rules across its parts goes
    /// (chunk 5.8c): its canonical text, which isn't read while compiling,
    /// so its rules can't be checked against it.
    Unread(String),
}

/// Checks a constant against the type it goes into.
pub fn check(table: &dyn TypeView, expected: &Ty, value: &Const) -> Result<(), Problem> {
    if let Ty::Opaque(name, args) = expected
        && args.is_empty()
    {
        return standard_object(name, value);
    }
    if expected.has_error() || matches!(expected, Ty::Var(_) | Ty::Opaque(..)) {
        return Ok(());
    }
    match value {
        Const::Other | Const::Path(_) => return Ok(()),
        Const::None => {
            return if table.is_nullable(expected) {
                Ok(())
            } else {
                Err(Problem::Kind { fix: None })
            };
        }
        Const::List(items) => {
            let structure = table.structure(expected);
            let element = match &structure {
                Ty::List(element) | Ty::Set(element) => element,
                // `[]` where a map is expected is an empty map (area 19,
                // "Sets and maps"); a map literal with entries is still area
                // 2's to design.
                Ty::Map(..) if items.is_empty() => &Ty::Error,
                _ => {
                    return Err(Problem::Kind { fix: None });
                }
            };
            for item in items {
                check(table, element, item)?;
            }
            if matches!(structure, Ty::Set(_))
                && let Some(twice) = repeated(items)
            {
                return Err(Problem::Builtin(format!(
                    "has {twice} twice, and a set holds each value once"
                )));
            }
            for (host, rules) in table.rule_chain(expected) {
                check_rules(rules, value).map_err(|broken| Problem::Broken(host, broken))?;
            }
            return vector_holding(table, expected, items);
        }
        _ => {}
    }
    let Some(base) = table.base_prim(expected) else {
        let fix = match (value, expected.strip_nullable()) {
            (Const::Str(text), Ty::Named(name)) => table
                .variants(name)
                .and_then(|variants| variants.iter().find(|v| v.name == *text))
                .map(|variant| format!("{name}.{}", variant.name)),
            _ => None,
        };
        return Err(Problem::Kind { fix });
    };
    let representation = base.representation();
    // A percentage is written with `%` (area 19): a bare number could be
    // 0.2 or 20.
    if representation == Prim::Percentage {
        match value {
            Const::Percentage(_) if value.percentage().is_some() => {}
            Const::Number { value: number, .. } => {
                return Err(Problem::PercentRequired(*number));
            }
            _ => return Err(Problem::Kind { fix: None }),
        }
        for (host, rules) in table.rule_chain(expected) {
            check_rules(rules, value).map_err(|broken| Problem::Broken(host, broken))?;
        }
        return Ok(());
    }
    // A quantity is always made with its unit (area 19): a bare number is
    // the mistake its type exists to prevent. A suffix that names no unit
    // is reported where the literal is read.
    if let Prim::Quantity(kind) = representation {
        match value {
            Const::Number { .. } | Const::TooLong => return Err(Problem::UnitRequired(kind)),
            Const::Quantity(_) => match value.quantity() {
                None => return Ok(()),
                Some((unit, _)) if unit.kind != kind => return Err(Problem::Kind { fix: None }),
                Some(_) => {}
            },
            _ => return Err(Problem::Kind { fix: None }),
        }
        for (host, rules) in table.rule_chain(expected) {
            check_rules(rules, value).map_err(|broken| Problem::Broken(host, broken))?;
        }
        return Ok(());
    }
    let fits = match value {
        Const::Number { decimal: false, .. } => representation == Prim::Int,
        Const::Number { decimal: true, .. } => representation == Prim::Decimal,
        // Instants and dates are written in ISO 8601, durations only with
        // a unit (area 10), and a `ZonedTime` is never a literal. Text
        // written in source may stand for text for a person, which a test
        // compares formatted text with (area 10).
        Const::Str(_) => matches!(
            representation,
            Prim::Text | Prim::Uuid | Prim::Instant | Prim::CalendarDate | Prim::PresentationText
        ),
        Const::Duration(_) => representation == Prim::Duration,
        Const::TooLong => representation.is_number(),
        Const::Bool(_) => representation == Prim::Bool,
        _ => false,
    };
    if !fits {
        let fix = match value {
            Const::Number {
                value,
                decimal: false,
            } if representation == Prim::Decimal => {
                let places = table
                    .rule_chain(expected)
                    .iter()
                    .find_map(|(_, rules)| rules.scale)
                    .unwrap_or(1)
                    .max(1);
                Some(format!(
                    "{}.{}",
                    value.mantissa,
                    "0".repeat(places as usize)
                ))
            }
            _ => None,
        };
        return Err(Problem::Kind { fix });
    }
    check_builtin(base, value).map_err(Problem::Builtin)?;
    if representation != base {
        check_builtin(representation, value).map_err(Problem::Builtin)?;
    }
    if representation == Prim::Int {
        check_builtin(Prim::Int, value).map_err(Problem::Builtin)?;
    }
    for (host, rules) in table.rule_chain(expected) {
        check_rules(rules, value).map_err(|broken| Problem::Broken(host, broken))?;
    }
    Ok(())
}

/// A literal where a standard object type goes (chunk 5.8c): only text,
/// its canonical text, which the type's contract reads where the value is
/// made, and only for a type with one text form and no rules across its
/// parts, since the checker can't read the text to check them; anything
/// else is the wrong kind of value, never a value kept as it was written.
fn standard_object(name: &str, value: &Const) -> Result<(), Problem> {
    let Some(entry) = jadpo_std::standard::standard_type(name) else {
        return Ok(());
    };
    if !matches!(
        entry.holds,
        jadpo_std::standard::Holds::Object(_) | jadpo_std::standard::Holds::Range
    ) {
        return Ok(());
    }
    match value {
        Const::Other | Const::Path(_) | Const::None => Ok(()),
        Const::Str(_) if entry.text.is_none() => Err(Problem::Kind { fix: None }),
        Const::Str(_) if !jadpo_std::standard::across::rules(name).is_empty() => {
            Err(Problem::Unread(name.to_string()))
        }
        Const::Str(_) => Ok(()),
        _ => Err(Problem::Kind { fix: None }),
    }
}

/// A vector written out against what its type holds beyond its length
/// (chunk 5.8c): items within a half-precision float where its index is
/// one, and not all zeros where it compares by cosine.
pub fn vector_holding(table: &dyn TypeView, expected: &Ty, items: &[Const]) -> Result<(), Problem> {
    use jadpo_std::standard::ai;
    let chain = table.rule_chain(expected);
    if !chain
        .iter()
        .any(|(_, rules)| rules.standard.is_some_and(ai::rounds))
    {
        return Ok(());
    }
    let mut floats = Vec::with_capacity(items.len());
    for item in items {
        let Const::Number { value, .. } = item else {
            return Ok(());
        };
        let Some(float) = ai::float(&value.to_string()).and_then(|kept| kept.parse::<f32>().ok())
        else {
            return Ok(());
        };
        floats.push(float);
    }
    let dimensions = chain.iter().find_map(|(_, rules)| rules.dimensions);
    let metric = chain
        .iter()
        .find_map(|(_, rules)| rules.distance)
        .unwrap_or(jadpo_std::VectorDistance::Cosine);
    let Some((key, _)) = ai::holding(&floats, dimensions, metric) else {
        return Ok(());
    };
    // The type that says so: the one writing `distance:` or
    // `dimensions:`, or `Vector` itself, which compares by cosine unless
    // told otherwise.
    let host = chain
        .iter()
        .find(|(_, rules)| match key {
            jadpo_std::RuleKey::Distance => rules.distance.is_some(),
            _ => rules.dimensions.is_some(),
        })
        .or_else(|| {
            chain
                .iter()
                .find(|(_, rules)| rules.standard.is_some_and(ai::rounds))
        })
        .map_or_else(String::new, |(host, _)| host.clone());
    let (rule, why) = match key {
        jadpo_std::RuleKey::Distance => (
            ai::COSINE_WRITTEN,
            "is a vector of zeros, which has no direction to compare by cosine",
        ),
        jadpo_std::RuleKey::Min => (
            ai::HALF_MIN_WRITTEN,
            "has an item below -65504, which its half-precision index can't hold",
        ),
        _ => (
            ai::HALF_MAX_WRITTEN,
            "has an item above 65504, which its half-precision index can't hold",
        ),
    };
    Err(Problem::Broken(
        host,
        jadpo_std::Broken {
            key,
            rule: rule.to_string(),
            why: why.to_string(),
        },
    ))
}

/// The first value a list literal gives twice, as written for a reason
/// line (area 2, question 5).
pub fn repeated(items: &[Const]) -> Option<String> {
    let same = |a: &Const, b: &Const| match (a, b) {
        (Const::Number { value: x, .. }, Const::Number { value: y, .. }) => x.compare(*y).is_eq(),
        (Const::Other | Const::TooLong, _) | (_, Const::Other | Const::TooLong) => false,
        _ => a == b,
    };
    items.iter().enumerate().find_map(|(i, item)| {
        items[..i]
            .iter()
            .any(|earlier| same(earlier, item))
            .then(|| match item {
                Const::Number { value, .. } => format!("`{value}`"),
                Const::Str(text) => format!("`\"{text}\"`"),
                Const::Bool(value) => format!("`{value}`"),
                Const::Duration(text) => format!("`{text}`"),
                Const::Path(names) => format!("`{}`", names.join(".")),
                _ => "a value".to_string(),
            })
    })
}

/// What a type is, for a reason line: "built on `Int`", "an enum".
pub fn what(table: &dyn TypeView, ty: &Ty) -> String {
    match ty.strip_nullable() {
        Ty::Prim(prim) if prim.is_representation() => "a built-in type".to_string(),
        Ty::Prim(prim) => format!("built on `{}`", prim.representation().name()),
        Ty::List(_) => "a list".to_string(),
        Ty::Set(_) => "a set".to_string(),
        Ty::Map(..) => "a map".to_string(),
        Ty::Named(name) => match table.def(name) {
            Some(Def::Enum { .. }) => "an enum".to_string(),
            Some(Def::Fields { .. }) => "an object with fields".to_string(),
            _ => match (table.base_prim(ty), table.structure(ty)) {
                (Some(prim), _) => format!("built on `{}`", prim.name()),
                (None, Ty::List(_)) => "a list".to_string(),
                (None, Ty::Set(_)) => "a set".to_string(),
                (None, Ty::Map(..)) => "a map".to_string(),
                (None, _) => "a named type".to_string(),
            },
        },
        _ => match table.base_prim(ty) {
            Some(prim) => format!("built on `{}`", prim.name()),
            None => "a different type".to_string(),
        },
    }
}

/// The most fields a shape names before it stops at `…`.
const SHAPE_FIELDS: usize = 8;

/// An object type a value is wanted for, as a mismatch shows it.
pub(crate) struct ObjectWanted {
    /// `PersonName { given: GivenName, family: FamilyName? }`.
    pub shape: String,
    /// The standard type's name, when it is one, for `jadpo help`.
    pub standard: Option<String>,
    /// Its name.
    name: String,
    /// The field or named type built from the object, as in
    /// `Book.author(PersonName { ... })`; `None` when the object itself is
    /// what is wanted.
    wrapper: Option<String>,
    /// Each field a value must give, with its declared type.
    required: Vec<(String, Ty)>,
}

impl ObjectWanted {
    fn wrapped(&self, object: String) -> String {
        match &self.wrapper {
            Some(wrapper) => format!("{wrapper}({object})"),
            None => object,
        }
    }

    /// The value written out, with what each field needs left to fill in.
    pub(crate) fn skeleton(&self) -> String {
        let name = &self.name;
        if self.required.is_empty() {
            return self.wrapped(format!("{name} {{ ... }}"));
        }
        let fields: Vec<String> = self
            .required
            .iter()
            .map(|(field, _)| format!("{field}: ..."))
            .collect();
        self.wrapped(format!("{name} {{ {} }}", fields.join(", ")))
    }

    /// The value `written`, a text literal, makes when it fits the one field
    /// the object must have: an edit that checks as it stands.
    fn holding(&self, table: &dyn TypeView, written: &str) -> Option<String> {
        let [(field, declared)] = self.required.as_slice() else {
            return None;
        };
        let text = written.strip_prefix('"')?.strip_suffix('"')?;
        if text.contains('\\') {
            return None;
        }
        check(table, declared, &Const::Str(text.to_string())).ok()?;
        Some(self.wrapped(format!("{} {{ {field}: {written} }}", self.name)))
    }
}

/// `expected` as an object a value could be written for, when it is one: an
/// object type, or a field or named type built on one.
pub(crate) fn object_wanted(table: &dyn TypeView, expected: &Ty) -> Option<ObjectWanted> {
    // A standard object type is opaque, its fields in its data entry.
    let (name, fields) = match table.structure(expected) {
        Ty::Named(name) if table.base(&name) == Some(FieldsBase::Object) => {
            let fields = table.fields(&name)?;
            (name, fields)
        }
        Ty::Opaque(name, args) if args.is_empty() => {
            let fields = standard_fields(&name)?;
            (name, fields)
        }
        _ => return None,
    };
    let shown = |field: &FieldDef| {
        let nullable = table.is_nullable(&field.declared);
        let mark = if field.optional && !nullable { "?" } else { "" };
        format!("{}: {}{mark}", field.name, field.declared)
    };
    let mut parts: Vec<String> = fields.iter().take(SHAPE_FIELDS).map(shown).collect();
    if fields.len() > SHAPE_FIELDS {
        parts.push("…".to_string());
    }
    let required = fields
        .iter()
        .filter(|field| {
            !field.optional && field.default.is_none() && !table.is_nullable(&field.declared)
        })
        .map(|field| (field.name.clone(), field.declared.clone()))
        .collect();
    // The outermost type a value is built through: a field whose declared
    // type is itself a field is built through that field.
    let mut wrapper = None;
    let mut current = expected.strip_nullable().clone();
    for _ in 0..8 {
        match &current {
            Ty::Field(host, path) => {
                wrapper = Some(current.to_string());
                let Some(field) = table.field(host, path) else {
                    break;
                };
                let declared = field.declared.strip_nullable().clone();
                if !matches!(declared, Ty::Field(..)) {
                    break;
                }
                current = declared;
            }
            Ty::Named(named) if *named != name => {
                wrapper = Some(current.to_string());
                break;
            }
            _ => break,
        }
    }
    Some(ObjectWanted {
        shape: format!("{name} {{ {} }}", parts.join(", ")),
        standard: is_standard(&name).then(|| name.clone()),
        name,
        wrapper,
        required,
    })
}

fn is_standard(name: &str) -> bool {
    jadpo_std::standard::STANDARD_TYPES
        .iter()
        .any(|entry| entry.name == name)
}

/// The standard type `ty` is, or is built on, when there is one.
pub(crate) fn standard_behind(table: &dyn TypeView, ty: &Ty) -> Option<String> {
    let mut current = ty.strip_nullable().clone();
    for _ in 0..8 {
        let next = match &current {
            Ty::Field(host, path) => table.field(host, path)?.declared.strip_nullable().clone(),
            Ty::Named(name) | Ty::Opaque(name, _) if is_standard(name) => {
                return Some(name.clone());
            }
            Ty::Named(name) => match table.def(name)? {
                Def::Scalar { parent, .. } => parent.strip_nullable().clone(),
                _ => return None,
            },
            _ => return None,
        };
        current = next;
    }
    None
}

/// Where a next step says the standard type is explained: `jadpo help`
/// works offline (area 14; chunk D16).
pub(crate) fn help_for(standard: Option<&str>) -> String {
    standard.map_or_else(String::new, |name| {
        format!(", and see `jadpo help {name}` for the type")
    })
}

/// The error for a literal that can't be `expected`: `type.mismatch` for
/// the wrong kind of value, `type.invalid_literal` for a broken rule.
/// `constructor` names the type being constructed, whose wrong-kind error is
/// `type.constructor_input`.
pub fn diagnostic(
    table: &dyn TypeView,
    problem: Problem,
    written: &str,
    expected: &Ty,
    span: Span,
    constructor: bool,
) -> Diagnostic {
    let value = describe(written);
    match problem {
        Problem::Kind { fix } => {
            let reason = if written == "none" {
                format!("`{expected}` can't be `none`; only a type written with `?` can.")
            } else {
                format!("This is {value}, and `{expected}` is {}.", what(table, expected))
            };
            let headline = if constructor {
                Diagnostic::new(
                    &rule::TYPE_CONSTRUCTOR_INPUT,
                    format!("`{expected}` can't be built from `{written}`"),
                )
            } else {
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{written}` can't be used where `{expected}` is required"),
                )
            };
            // An object: its shape, and the value written out, so the first
            // error is enough to write the next attempt (chunk A14).
            let object = (!constructor && written != "none" && fix.is_none())
                .then(|| object_wanted(table, expected))
                .flatten();
            let reason = match &object {
                Some(object) => format!(
                    "This is {value}, and `{expected}` is an object: `{}`.",
                    object.shape
                ),
                None => reason,
            };
            let problem = headline.at(span.clone(), "the wrong kind of value").reason(reason);
            match (fix, object) {
                (Some(fix), _) => problem
                    .next(format!("Write `{fix}`"))
                    .edit(span, fix),
                (None, Some(object)) => match object.holding(table, written) {
                    Some(value) => problem
                        .next(format!(
                            "Write `{value}`{}",
                            help_for(object.standard.as_deref())
                        ))
                        .edit(span, value),
                    None => problem.next(format!(
                        "Write `{}`{}",
                        object.skeleton(),
                        help_for(object.standard.as_deref())
                    )),
                },
                (None, None) => problem.next(format!(
                    "Use a `{expected}` value here{}",
                    help_for(standard_behind(table, expected).as_deref())
                )),
            }
        }
        Problem::UnitRequired(kind) => unit_required(kind, written, &expected.to_string(), span),
        Problem::Unread(standard) => Diagnostic::new(
            &rule::TYPE_INVALID_LITERAL,
            format!("`{written}` can't be checked as a `{expected}` while compiling"),
        )
        .at(span, "canonical text")
        .reason(format!(
            "A `{standard}` has rules across its parts, which every literal is checked against while compiling; text written where one goes is its canonical text, which isn't read until the program runs, so its rules can't be checked."
        ))
        .next(format!(
            "Write it out as `{standard} {{ ... }}`, or read the text with `text.parse(..., as: {standard})` and `attempt`"
        )),
        Problem::PercentRequired(number) => {
            let headline = if constructor {
                Diagnostic::new(
                    &rule::TYPE_CONSTRUCTOR_INPUT,
                    format!("`{expected}` can't be built from `{written}`"),
                )
            } else {
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{written}` can't be used where `{expected}` is required"),
                )
            };
            headline
                .at(span, "a bare number")
                .reason(format!(
                    "`{expected}` is a percentage, written with `%`, so a bare number could be read two ways: as a ratio, where 0.2 is 20%, or as a percentage already, where 20 is 20%."
                ))
                .next(percent_readings(number, written))
        }
        Problem::Builtin(why) => {
            let base = table
                .base_prim(expected)
                .map_or_else(|| expected.to_string(), |prim| prim.name().to_string());
            Diagnostic::new(
                &rule::TYPE_INVALID_LITERAL,
                format!("`{written}` isn't a valid `{expected}`"),
            )
            .at(span, format!("not a valid `{base}`"))
            .reason(format!(
                "The value {why}; a literal is checked while compiling."
            ))
            .next(format!("Write a valid `{base}` value"))
        }
        // `Html` written in code must already be safe: its construction
        // makes any other text safe, but a literal is checked while
        // compiling, so it says what it means (area 19, chunk 5.5o).
        Problem::Broken(_, broken)
            if broken.key == jadpo_std::RuleKey::Format
                && jadpo_std::standard::markup::sanitises(&broken.rule) =>
        {
            let standard = broken.rule;
            let kept = broken
                .why
                .split_once("keeps `")
                .map(|(_, kept)| kept.trim_end_matches('`'));
            let next = match kept {
                Some(kept) if !kept.is_empty() => {
                    format!("Write it as the sanitiser keeps it: `{kept}`")
                }
                _ => "Write only the elements and attributes the sanitiser keeps".to_string(),
            };
            Diagnostic::new(
                &rule::TYPE_INVALID_LITERAL,
                format!("`{written}` isn't safe `{expected}` as written"),
            )
            .at(span, format!("not what `{standard}`'s sanitiser keeps"))
            .reason(format!(
                "The value {}. An `{standard}` keeps only the allowed elements and attributes, with no scripts, no event handlers and links only to `https:` and `mailto:`; a literal is checked while compiling, so it is written as it is kept.",
                broken.why
            ))
            .next(next)
        }
        // A standard type held as text: its own check, as a built-in
        // type's is (area 19, chunk 5.5i).
        Problem::Broken(_, broken)
            if broken.key == jadpo_std::RuleKey::Format
                && jadpo_std::standard::text_rules(&broken.rule).is_some() =>
        {
            let standard = broken.rule;
            Diagnostic::new(
                &rule::TYPE_INVALID_LITERAL,
                format!("`{written}` isn't a valid `{expected}`"),
            )
            .at(span, format!("not a valid `{standard}`"))
            .reason(format!(
                "The value {}: its pattern and check digits accept only `{standard}`'s canonical text, and a literal is checked while compiling.",
                broken.why
            ))
            .next(format!("Write a valid `{standard}` in its canonical text"))
        }
        // A span's link, written so the sanitiser keeps it (chunk 5.5o2):
        // said in words, not as its pattern.
        Problem::Broken(_, broken)
            if broken.key == jadpo_std::RuleKey::Pattern
                && broken
                    .rule
                    .contains(jadpo_std::standard::rich_text::LINK_PATTERN) =>
        {
            Diagnostic::new(
                &rule::TYPE_INVALID_LITERAL,
                format!("`{written}` isn't a link a `RichText` can hold"),
            )
            .at(span, "not an `https:` or `mailto:` address")
            .reason(
                "A span links only to an `https:` URL with a host or a `mailto:` address, written so the HTML sanitiser keeps it as it is, since `markup.rich_to_html` writes every link out; a literal is checked while compiling.",
            )
            .next("Write an `https:` or `mailto:` address, such as `https://example.com/`")
        }
        Problem::Broken(host, broken) => Diagnostic::new(
            &rule::TYPE_INVALID_LITERAL,
            format!("`{written}` isn't a valid `{expected}`"),
        )
        .at(span, format!("breaks `{}`", broken.rule))
        .reason(format!(
            "`{host}` declares `{}`, and the value {}. A literal is checked against every rule while compiling.",
            broken.rule, broken.why
        ))
        .next(format!("Use a value that `{expected}` allows")),
    }
}

/// `type.unit_required` (area 19): `written`, a bare number, where a
/// quantity of `kind` goes, as type `expected`. Never declinable.
pub fn unit_required(
    kind: jadpo_std::QuantityKind,
    written: &str,
    expected: &str,
    span: Span,
) -> Diagnostic {
    let base = kind.base_unit();
    let literal = written.parse::<f64>().is_ok() || written.starts_with('-');
    let made = format!("units.of({written}, {}.{})", kind.unit_enum(), base.name);
    let next = if literal && base.suffix {
        format!(
            "Write its unit after the number, such as `{written}{}`, or make it with `{made}` in any of `{}`'s units",
            base.code,
            kind.unit_enum()
        )
    } else {
        format!(
            "Make it with its unit, `{made}`, in any of `{}`'s units",
            kind.unit_enum()
        )
    };
    let an = if kind.name().starts_with(['A', 'E', 'I', 'O', 'U']) {
        "An"
    } else {
        "A"
    };
    Diagnostic::new(
        &rule::TYPE_UNIT_REQUIRED,
        format!("`{expected}` needs a unit, and `{written}` is a bare number"),
    )
    .at(span, "no unit")
    .reason(format!(
        "{an} `{}` is always made with its unit, so a number alone could be any {}: a number without its unit is the mistake the type exists to prevent.",
        kind.name(),
        kind.name().to_lowercase()
    ))
    .next(next)
}

/// `type.unit_unknown` (area 19): a number written with a suffix that is
/// no duration's or quantity's unit, naming each unit it misses only by
/// case (`5mw`, between `mW` and `MW`).
pub fn unit_unknown(written: &str, span: Span) -> Diagnostic {
    let suffix = jadpo_std::quantity::suffix_of(written);
    let near = jadpo_std::quantity::near_misses(suffix);
    let number = written.strip_suffix(suffix).unwrap_or(written);
    if near.is_empty() {
        return Diagnostic::new(
            &rule::TYPE_UNIT_UNKNOWN,
            format!("`{suffix}` isn't a unit, so `{written}` is no value"),
        )
        .at(span, "no such unit")
        .reason(format!(
            "A number's suffix is a duration's unit (`ms`, `s`, `m`, `h`, `d`) or a quantity's, as SI writes it and case matters; no unit is written `{suffix}`. Metres are `metre`, since `m` is minutes, and inches `inch`, since `in` is a keyword."
        ))
        .next("Write a unit Jadpo knows, as the editor offers after the number, or make the value with `units.of`");
    }
    let named: Vec<String> = near
        .iter()
        .map(|unit| format!("`{number}{}` ({})", unit.code, unit.meaning))
        .collect();
    let mut problem = Diagnostic::new(
        &rule::TYPE_UNIT_UNKNOWN,
        format!(
            "`{written}` has no unit `{suffix}`: did you mean {}?",
            crate::or_list(&named)
        ),
    )
    .at(span.clone(), "unit in the wrong case")
    .reason(format!(
        "Units are written as SI writes them, and case matters: {}.",
        crate::and_list(
            &near
                .iter()
                .map(|unit| format!("`{}` is {}", unit.code, unit.meaning))
                .collect::<Vec<_>>()
        )
    ));
    problem = match near.as_slice() {
        [only] => problem
            .next(format!("Write `{number}{}`", only.code))
            .edit(span, format!("{number}{}", only.code)),
        _ => problem.next(format!(
            "Write {}, whichever is meant",
            crate::or_list(&named)
        )),
    };
    problem
}

/// The next step for a bare number where a percentage goes: both
/// readings, with no edit guessing which (area 19).
pub fn percent_readings(number: jadpo_std::Num, written: &str) -> String {
    let as_ratio = jadpo_std::percentage::canonical(number.mantissa, number.scale);
    format!(
        "Write it with `%`: `{as_ratio}` if `{written}` is a ratio, or `{written}%` if it is already a percentage"
    )
}

/// A literal in the reader's words.
fn describe(written: &str) -> &'static str {
    if written == "none" {
        "`none`"
    } else if written.starts_with('"') {
        "text"
    } else if written == "true" || written == "false" {
        "a `Bool`"
    } else if written.starts_with('[') {
        "a list"
    } else if written.ends_with('%') {
        "a percentage"
    } else if written.contains('.') {
        "a decimal number"
    } else if written.ends_with(|c: char| c.is_ascii_alphabetic())
        && super::rules::is_duration_suffix(jadpo_std::quantity::suffix_of(written))
    {
        "a duration"
    } else if written.ends_with(|c: char| c.is_ascii_alphabetic()) {
        "a quantity"
    } else {
        "a whole number"
    }
}
