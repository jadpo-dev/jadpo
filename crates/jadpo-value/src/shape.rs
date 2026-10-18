//! Building a value of an object or variant from its fields, the way a
//! body builds one and a request's input is decoded (area 2): each field
//! in its shape's declared order, those left out taking what they hold
//! when absent, and an `Input` remembering the optional fields it was
//! built without (area 6, patch). Shared by the interpreter and the
//! runtime's JSON codecs.

use std::rc::Rc;

use jadpo_std::types::ShapePlan;

use crate::{Record, Value, Variant};

/// A shape with the names a value holds, ready to build values from.
#[derive(Debug)]
pub struct Shape {
    ty: Rc<str>,
    variant: Option<(Rc<str>, u32)>,
    /// Each field's name and what a value built without it holds.
    fields: Vec<(Rc<str>, Option<Value>)>,
    /// The places of the optional `Input` fields without a default: a
    /// value built without one remembers it was left out (area 6, patch).
    omittable: Vec<u32>,
    /// The places of those whose type isn't nullable: `none` given to one
    /// is the same as leaving it out.
    none_left_out: Vec<u32>,
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
                        field.absent.as_ref().map(Value::constant),
                    )
                })
                .collect(),
            omittable: plan
                .fields
                .iter()
                .enumerate()
                .filter(|(_, field)| field.omittable)
                .map(|(at, _)| at as u32)
                .collect(),
            none_left_out: plan
                .fields
                .iter()
                .enumerate()
                .filter(|(_, field)| field.none_left_out)
                .map(|(at, _)| at as u32)
                .collect(),
        }
    }

    /// The object's type or entity, or the variant's enum.
    pub fn ty(&self) -> &str {
        &self.ty
    }

    /// How many fields it has.
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Field `slot`'s name, and whether a value built without it holds
    /// something (an optional or defaulted `Input` field).
    pub fn field(&self, slot: usize) -> Option<(&str, bool)> {
        self.fields
            .get(slot)
            .map(|(name, absent)| (&**name, absent.is_some()))
    }

    /// Whether field `slot` is an optional `Input` field without a default,
    /// so `null` sent for it means `none`.
    pub fn omittable(&self, slot: usize) -> bool {
        self.omittable.contains(&(slot as u32))
    }

    /// The value built from `given` (each with its place among the shape's
    /// fields), the rest taking what they hold when left out; `None` when
    /// a field is missing that has nothing to hold.
    pub fn build(&self, given: impl IntoIterator<Item = (Option<u32>, Value)>) -> Option<Value> {
        let mut values: Vec<Option<Value>> = vec![None; self.fields.len()];
        for (slot, value) in given {
            if value == Value::None && slot.is_some_and(|slot| self.none_left_out.contains(&slot)) {
                continue;
            }
            if let Some(place) = slot.and_then(|slot| values.get_mut(slot as usize))
                && place.is_none()
            {
                *place = Some(value);
            }
        }
        let omitted: Vec<u32> = self
            .omittable
            .iter()
            .copied()
            .filter(|at| values.get(*at as usize).is_some_and(Option::is_none))
            .collect();
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
                omitted: (!omitted.is_empty()).then(|| Rc::from(omitted)),
            })),
        })
    }
}
