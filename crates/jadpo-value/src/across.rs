//! Running an object type's rules across its fields (area 2, "Rules
//! across fields") on a value of it: the one implementation the
//! interpreter, compiled code and the codecs share, so a rule broken by an
//! object literal is the rule `json.read` and a request's input break.
//!
//! A condition reads only the object's fields, literals and what the
//! standard operations give, so it needs nothing but the value. A condition
//! that can't be worked out (an `Int` past 64 bits) counts as broken: the
//! value is refused rather than let through unchecked.

use std::cmp::Ordering;

use jadpo_std::across::{CrossRule, RuleExpr, RuleOp, RulePattern};
use jadpo_std::standard::across::{Given, Known, Parts};

use crate::{Fault, Record, Value, ops};

/// Whether `rule` holds for an object whose fields, in the order its type
/// declares them, are `fields`.
pub fn holds(rule: &CrossRule, fields: &[Value]) -> bool {
    let mut locals = fields.to_vec();
    let wanted = (rule.locals as usize).max(fields.len());
    locals.resize(wanted, Value::None);
    matches!(eval(&rule.check, &mut locals), Ok(Value::Bool(true)))
}

fn eval(expr: &RuleExpr, locals: &mut Vec<Value>) -> Result<Value, Fault> {
    match expr {
        RuleExpr::Local(at) => locals.get(*at as usize).cloned().ok_or(Fault::Shape),
        RuleExpr::Constant(constant) => Ok(Value::constant(constant)),
        RuleExpr::Field { receiver, name } => match eval(receiver, locals)? {
            Value::Record(record) => record.get(name).cloned().ok_or(Fault::Shape),
            Value::Variant(variant) => variant.get(name).cloned().ok_or(Fault::Shape),
            _ => Err(Fault::Shape),
        },
        RuleExpr::List(items) => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.push(eval(item, locals)?);
            }
            Ok(Value::list(values))
        }
        RuleExpr::Binary {
            op: RuleOp::And,
            lhs,
            rhs,
            ..
        } => match eval(lhs, locals)? {
            Value::Bool(false) => Ok(Value::Bool(false)),
            Value::Bool(true) => eval(rhs, locals),
            _ => Err(Fault::Shape),
        },
        RuleExpr::Binary {
            op: RuleOp::Or,
            lhs,
            rhs,
            ..
        } => match eval(lhs, locals)? {
            Value::Bool(true) => Ok(Value::Bool(true)),
            Value::Bool(false) => eval(rhs, locals),
            _ => Err(Fault::Shape),
        },
        RuleExpr::Binary {
            op,
            lhs,
            rhs,
            rounding,
        } => {
            let left = eval(lhs, locals)?;
            let right = eval(rhs, locals)?;
            let ordered = |wanted: fn(Ordering) -> bool| {
                ops::order(&left, &right).map(|ordering| Value::Bool(wanted(ordering)))
            };
            match op {
                RuleOp::Equal => Ok(Value::Bool(ops::equal(&left, &right))),
                RuleOp::NotEqual => Ok(Value::Bool(!ops::equal(&left, &right))),
                RuleOp::Less => ordered(Ordering::is_lt),
                RuleOp::LessEqual => ordered(Ordering::is_le),
                RuleOp::Greater => ordered(Ordering::is_gt),
                RuleOp::GreaterEqual => ordered(Ordering::is_ge),
                RuleOp::Add => ops::add(&left, &right),
                RuleOp::Subtract => ops::subtract(&left, &right),
                RuleOp::Multiply => ops::multiply_rounded(&left, &right, *rounding),
                RuleOp::Divide => ops::divide_rounded(&left, &right, *rounding),
                RuleOp::Remainder => ops::remainder(&left, &right),
                RuleOp::In => ops::contains(&right, &left)
                    .map(Value::Bool)
                    .ok_or(Fault::Shape),
                RuleOp::And | RuleOp::Or => Err(Fault::Shape),
            }
        }
        RuleExpr::Not(inner) => match eval(inner, locals)? {
            Value::Bool(value) => Ok(Value::Bool(!value)),
            _ => Err(Fault::Shape),
        },
        RuleExpr::Negate(inner) => ops::negate(&eval(inner, locals)?),
        RuleExpr::Std { op, args } => {
            let mut values = Vec::with_capacity(args.len());
            for arg in args {
                values.push(eval(arg, locals)?);
            }
            // A condition has no inline function, and no operation it may
            // call reads the application's locales.
            crate::run(*op, &values, None, &mut |_| None)
        }
        RuleExpr::Match { subject, arms } => {
            let value = eval(subject, locals)?;
            for arm in arms {
                let (matched, bound) = match (&arm.pattern, &value) {
                    (RulePattern::None, Value::None) => (true, None),
                    (RulePattern::None, _) => (false, None),
                    (RulePattern::Some, Value::None) => (false, None),
                    (RulePattern::Some, other) => (true, Some(other.clone())),
                    (RulePattern::True, Value::Bool(flag)) => (*flag, None),
                    (RulePattern::False, Value::Bool(flag)) => (!*flag, None),
                    (RulePattern::Variant(ordinal), Value::Variant(variant)) => {
                        (variant.ordinal == *ordinal, Some(value.clone()))
                    }
                    (RulePattern::Literal(constant), other) => {
                        (ops::equal(other, &Value::constant(constant)), None)
                    }
                    (RulePattern::Wildcard, _) => (true, None),
                    _ => (false, None),
                };
                if !matched {
                    continue;
                }
                if let (Some(binding), Some(bound)) = (arm.binding, bound)
                    && let Some(slot) = locals.get_mut(binding as usize)
                {
                    *slot = bound;
                }
                return eval(&arm.body, locals);
            }
            Err(Fault::Shape)
        }
        // A standard object's rule across its parts (area 19, chunk
        // 5.5t2): the object's fields are its parts in their declared
        // order, read by the rule's one check.
        RuleExpr::Standard { ty, rule } => {
            let rule = jadpo_std::standard::across::rule(ty, rule).ok_or(Fault::Shape)?;
            let names = jadpo_std::standard::across::part_names(ty);
            let parts = Fields {
                ty,
                names: &names,
                values: locals,
            };
            Ok(Value::Bool(rule.holds(&parts)))
        }
    }
}

/// A standard object's parts, its fields by name, as its rules read them.
struct Fields<'a> {
    ty: &'a str,
    names: &'a [&'static str],
    values: &'a [Value],
}

impl Parts for Fields<'_> {
    fn part(&self, name: &str) -> Known<Given<'_>> {
        let standard = jadpo_std::standard::across::part_standard(self.ty, name);
        self.names
            .iter()
            .position(|listed| *listed == name)
            .and_then(|at| self.values.get(at))
            .map_or(Known::Unknown, |value| given(value, standard))
    }
}

impl Parts for Record {
    fn part(&self, name: &str) -> Known<Given<'_>> {
        let standard = jadpo_std::standard::across::part_standard(&self.ty, name);
        self.get(name)
            .map_or(Known::Unknown, |value| given(value, standard))
    }
}

/// What a standard rule reads of a part's value: `none` absent, and every
/// other value given as the kind it is, or given with no value a rule can
/// read, which decides nothing and so counts as broken. A literal holds a
/// frame rate or a local time written as text as that text; `standard`,
/// the standard type the part is built on, says how to read it.
fn given<'v>(value: &'v Value, standard: Option<&str>) -> Known<Given<'v>> {
    let read = match value {
        Value::None => return Known::Absent,
        Value::Text(text) if standard == Some(jadpo_std::standard::media::FRAME_RATE) => {
            jadpo_std::percentage::parse_fraction(text).map(|(n, d)| Given::Rate(n, d))
        }
        Value::Text(text) if standard == Some("LocalDateTime") => {
            jadpo_std::standard::across::local_at_text(text).map(Given::Local)
        }
        Value::Int(n) => Some(Given::Number(jadpo_std::Num::whole(i128::from(*n)))),
        Value::Decimal(d) => Some(Given::Number(jadpo_std::Num {
            mantissa: d.mantissa,
            scale: d.scale,
        })),
        Value::Quantity(q) => Some(Given::Quantity(q.kind, q.amount)),
        Value::Instant(ms) => Some(Given::Instant(*ms)),
        Value::Date(day) => Some(Given::Date(*day)),
        Value::Duration(ms) => Some(Given::Duration(*ms)),
        Value::Text(text) => Some(Given::Text(text)),
        Value::Bool(flag) => Some(Given::Bool(*flag)),
        Value::Variant(variant) => Some(Given::Variant {
            ordinal: variant.ordinal,
            code: jadpo_std::standard_type(&variant.ty)
                .and_then(jadpo_std::StandardType::codes)
                .and_then(|codes| codes.get(variant.ordinal as usize))
                .map_or("", |(_, code)| *code),
        }),
        Value::Record(record) if &*record.ty == "LocalDateTime" => {
            crate::calendar::local_date_time(value)
                .ok()
                .map(|(date, millis)| {
                    Given::Local(jadpo_std::standard::recurrence::LocalAt {
                        date,
                        second: millis / 1000,
                    })
                })
        }
        Value::Record(record) if &*record.ty == jadpo_std::standard::media::FRAME_RATE => {
            match (record.get("numerator"), record.get("denominator")) {
                (Some(Value::Int(n)), Some(Value::Int(d))) => Some(Given::Rate(*n, *d)),
                _ => None,
            }
        }
        _ => None,
    };
    Known::Given(read)
}

/// The first of a standard object's rules across its parts `value` breaks,
/// by name (area 19, chunk 5.5t2); `None` for a value that keeps them all,
/// or any other value.
pub fn broken_standard(value: &Value) -> Option<&'static str> {
    let Value::Record(record) = value else {
        return None;
    };
    jadpo_std::standard::across::first_broken(&record.ty, &**record).map(|rule| rule.name)
}
