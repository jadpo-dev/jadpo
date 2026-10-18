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

use crate::{Fault, Value, ops};

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
    }
}
