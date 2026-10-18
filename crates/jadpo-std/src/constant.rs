//! A value known while compiling (area 2, question 15), in the
//! representation the runtime holds it in (`jadpo-value`'s `Value`), as
//! plain data: a literal decoded for the type it goes into, a field's
//! default, or a value a plan fills a failure's field with. `jadpo-plan`
//! decodes each once; the interpreter turns each into a value per thread
//! and the code generator writes it into the program.

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Constant {
    Unit,
    None,
    Bool(bool),
    Int(i64),
    /// `mantissa × 10^-scale`, at the scale its type keeps.
    Decimal {
        mantissa: i128,
        scale: u32,
    },
    Text(String),
    /// Lowercase.
    Uuid(String),
    /// Milliseconds since 1970-01-01T00:00:00Z.
    Instant(i64),
    /// Days since 1970-01-01.
    Date(i32),
    /// Milliseconds.
    Duration(i64),
    /// A physical quantity: its kind and its amount in the kind's base
    /// unit, a mantissa at the kind's scale (area 19).
    Quantity {
        kind: crate::QuantityKind,
        amount: i128,
    },
    /// A percentage, as the fraction it is: `20%` is `mantissa` 20 at
    /// `scale` 2 (area 19).
    Percentage {
        mantissa: i128,
        scale: u32,
    },
    /// A variant of an enum, the `ordinal`th it declares, with a payload's
    /// fields in their declared order: a field's default.
    Variant {
        ty: String,
        ordinal: u32,
        name: String,
        fields: Vec<(String, Constant)>,
    },
}
