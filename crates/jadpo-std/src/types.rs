//! Type plans (docs/compiler-runtime.md §2.1 and §2.5): what a value of
//! each type is made of, as plain data `jadpo-plan` reads from the graph
//! once and both executors, the runtime's failures and the codecs read, so
//! each builds, checks and reads values the same way without a compiler
//! crate.
//!
//! - A [`ValidatorPlan`] is a type's rules along its chain, most specific
//!   first, and the built-in type at the bottom: what the runtime's
//!   validator checks a value against and the scale it keeps a decimal at.
//! - A [`ShapePlan`] is an object's, entity's or variant's fields in the
//!   order the type declares them, each with what an `Input` left without
//!   it holds; a variant's also says where it stands among its enum's
//!   variants, which is what sets order variants by (area 2, "Set order").
//! - A [`FailurePlan`] is a declared failure's code, kind, status, message
//!   and fields, each in its client or internal section (area 4).
//!
//! Plans are numbered, and a plan names another by its number: a field's
//! validator, a read's not-found failure.

use crate::{Builtin, Constant, Rules};

/// A type's rules ready to compile into a validator, by number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ValidatorId(pub u32);

/// An object's, entity's or variant's shape, by number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShapeId(pub u32);

/// A declared failure's plan, by number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FailureId(pub u32);

/// A type's rules along its chain.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ValidatorPlan {
    /// What the chain is built on; `None` for an object, an enum or a
    /// container.
    pub builtin: Option<Builtin>,
    /// Each declaration's rules along the chain, most specific first.
    pub rules: Vec<Rules>,
}

impl ValidatorPlan {
    /// The fixed scale a decimal of this type is kept at: the first
    /// `scale:` along the chain.
    pub fn scale(&self) -> Option<u32> {
        self.rules.iter().find_map(|rules| rules.scale)
    }
}

/// A failure's fields are sent to the caller or kept for the log (area 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Section {
    Client,
    Internal,
}

impl Section {
    pub fn word(self) -> &'static str {
        match self {
            Section::Client => "client",
            Section::Internal => "internal",
        }
    }
}

/// One field of an object, entity, variant or failure.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FieldPlan {
    /// Its identity key: `field PaymentOutcome.paid.receipt`.
    pub key: String,
    /// Its own name, as a value holds it: `receipt` for
    /// `PaymentOutcome.paid.receipt`.
    pub name: String,
    /// Its rules: the field's own, then its type's.
    pub validator: ValidatorId,
    /// What a value built without it holds: `none` for an optional
    /// `Input` field, its `default:` for one with a default (area 2).
    pub absent: Option<Constant>,
    /// An optional `Input` field without a `default:`: a value built
    /// without it remembers it was left out, which an update's `patch:`
    /// reads (area 6).
    pub omittable: bool,
    /// An omittable field whose type isn't nullable: `none` given to it is
    /// the same as leaving it out, since nothing could clear it (area 6,
    /// patch; chosen in chunk 3.10), so a patch keeps the stored value.
    pub none_left_out: bool,
    /// A failure field's section.
    pub section: Option<Section>,
}

/// A variant: its name and where it stands among its enum's variants.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VariantPlan {
    pub name: String,
    /// From 0, in the order the enum declares its variants.
    pub ordinal: u32,
}

/// What an object, entity or variant is made of.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ShapePlan {
    /// The object's type or entity, or the variant's enum, by its name now.
    pub ty: String,
    /// For a variant.
    pub variant: Option<VariantPlan>,
    /// In the order the type declares them.
    pub fields: Vec<FieldPlan>,
}

impl ShapePlan {
    /// The place of the field with identity key `key` among the shape's
    /// fields.
    pub fn slot(&self, key: &str) -> Option<u32> {
        place(&self.fields, |field| field.key == key)
    }

    /// The place of the field called `name`.
    pub fn slot_named(&self, name: &str) -> Option<u32> {
        place(&self.fields, |field| field.name == name)
    }
}

/// A declared failure (area 4).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FailurePlan {
    /// Its identity key: `type QuantityUnavailable`.
    pub key: String,
    /// Its name as written.
    pub name: String,
    /// Its public code.
    pub code: String,
    /// `FailureKind.conflict` as `conflict`.
    pub kind: Option<String>,
    /// The HTTP status its kind sets.
    pub status: Option<u16>,
    pub message: Option<String>,
    /// In the order the failure declares them, across its `client` and
    /// `internal` blocks.
    pub fields: Vec<FieldPlan>,
}

impl FailurePlan {
    /// The place of the field with identity key `key` among the failure's
    /// fields.
    pub fn slot(&self, key: &str) -> Option<u32> {
        place(&self.fields, |field| field.key == key)
    }
}

fn place(fields: &[FieldPlan], wanted: impl Fn(&FieldPlan) -> bool) -> Option<u32> {
    let at = fields.iter().position(wanted)?;
    u32::try_from(at).ok()
}
