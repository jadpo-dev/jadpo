//! The plan's values on one thread: its constants and shapes decoded into
//! the runtime's `Rc`-shared values, once per thread (values never cross
//! threads, §2.6), and the codecs that build a value of a type from its
//! parts the way a body builds one.

use std::rc::Rc;

use jadpo_graph::TypeRef;
use jadpo_plan::{Constant, ShapePlan, Types};
use jadpo_value::{Decimal, Record, Value, Variant};

/// A shape with the names a value holds, ready to build values from.
#[derive(Debug)]
pub(crate) struct Shape {
    ty: Rc<str>,
    variant: Option<(Rc<str>, u32)>,
    /// Each field's name and what a value built without it holds.
    fields: Vec<(Rc<str>, Option<Value>)>,
}

impl Shape {
    pub fn new(plan: &ShapePlan) -> Shape {
        Shape {
            ty: Rc::from(plan.ty.as_str()),
            variant: plan
                .variant
                .as_ref()
                .map(|variant| (Rc::from(variant.name.as_str()), variant.ordinal)),
            fields: plan
                .fields
                .iter()
                .map(|field| {
                    (
                        Rc::from(field.name.as_str()),
                        field.absent.as_ref().map(value),
                    )
                })
                .collect(),
        }
    }

    /// The value built from `given` (each with its place among the shape's
    /// fields), the rest taking what they hold when left out; `None` when
    /// a field is missing that has nothing to hold.
    pub fn build(&self, given: impl IntoIterator<Item = (Option<u32>, Value)>) -> Option<Value> {
        let mut values: Vec<Option<Value>> = vec![None; self.fields.len()];
        for (slot, value) in given {
            if let Some(place) = slot.and_then(|slot| values.get_mut(slot as usize))
                && place.is_none()
            {
                *place = Some(value);
            }
        }
        let fields = self
            .fields
            .iter()
            .zip(values)
            .map(|((name, absent), value)| {
                Some((Rc::clone(name), value.or_else(|| absent.clone())?))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(match &self.variant {
            Some((name, ordinal)) => Value::Variant(Rc::new(Variant {
                ty: Rc::clone(&self.ty),
                ordinal: *ordinal,
                name: Rc::clone(name),
                fields,
            })),
            None => Value::Record(Rc::new(Record {
                ty: Rc::clone(&self.ty),
                fields,
            })),
        })
    }
}

/// A constant as the runtime holds it.
pub(crate) fn value(constant: &Constant) -> Value {
    match constant {
        Constant::Unit => Value::Unit,
        Constant::None => Value::None,
        Constant::Bool(value) => Value::Bool(*value),
        Constant::Int(n) => Value::Int(*n),
        Constant::Decimal { mantissa, scale } => Value::Decimal(Decimal::new(*mantissa, *scale)),
        Constant::Text(text) => Value::text(text),
        Constant::Uuid(text) => Value::Uuid(Rc::from(text.as_str())),
        Constant::Instant(millis) => Value::Instant(*millis),
        Constant::Date(day) => Value::Date(*day),
        Constant::Time(millis) => Value::Time(*millis),
        Constant::Duration(millis) => Value::Duration(*millis),
        Constant::Variant {
            ty,
            ordinal,
            name,
            fields,
        } => Value::variant(
            ty,
            *ordinal,
            name,
            fields
                .iter()
                .map(|(name, field)| (Rc::from(name.as_str()), value(field)))
                .collect(),
        ),
    }
}

/// Values of a project's types built from their parts, as its bodies build
/// them: for tests and tools that pass a callable arguments.
#[derive(Clone, Copy, Debug)]
pub struct Codec<'a> {
    pub(crate) types: &'a Types,
}

impl Codec<'_> {
    /// An object of type `ty` from every one of its fields by name; `None`
    /// when `ty` isn't an object or a field is missing or unknown.
    pub fn object(&self, ty: &str, fields: Vec<(&str, Value)>) -> Option<Value> {
        let key = self.types.named(ty)?;
        self.build(&self.types.shape_plan(&key)?, fields)
    }

    /// Variant `variant` of enum `ty`, with a payload variant's fields by
    /// name.
    pub fn variant(&self, ty: &str, variant: &str, fields: Vec<(&str, Value)>) -> Option<Value> {
        let key = self.types.variant_named(ty, variant);
        self.types.graph().get(&key)?;
        self.build(&self.types.shape_plan(&key)?, fields)
    }

    /// A value of the named type `ty` from `text`, as a literal of it would
    /// be: a UUID, an instant or a date from its ISO text, text otherwise.
    pub fn text(&self, ty: &str, text: &str) -> Option<Value> {
        let key = self.types.named(ty)?;
        self.types
            .text(&TypeRef::Node(key), text)
            .map(|constant| value(&constant))
    }

    fn build(&self, plan: &ShapePlan, fields: Vec<(&str, Value)>) -> Option<Value> {
        if fields.len() != plan.fields.len() {
            return None;
        }
        let given = fields
            .into_iter()
            .map(|(name, value)| Some((Some(plan.slot_named(name)?), value)))
            .collect::<Option<Vec<_>>>()?;
        Shape::new(plan).build(given)
    }
}
