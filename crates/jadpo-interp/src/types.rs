//! What running a body needs to know about types, read from the graph
//! (docs/compiler-runtime.md §2.3): each type's rules along its chain as a
//! [`Validator`], what it is built on, the fields of each object and
//! variant in the order they are declared, and each failure's code, kind
//! and fields. Read once per type and kept; `jadpo-plan` takes this over
//! when it is built.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use jadpo_graph::body::Literal;
use jadpo_graph::{Base, Detail, FieldDefault, Graph, Key, Kind, Node, Rules, Section, TypeRef};
use jadpo_runtime::Failure;
use jadpo_std::Builtin;
use jadpo_value::{Validator, Value};

/// One field of an object, a variant or a failure.
#[derive(Clone, Debug)]
pub(crate) struct Field {
    pub key: Key,
    pub name: Rc<str>,
    pub section: Option<Section>,
    /// What an `Input` left without this field holds: `none` for an
    /// optional field, its `default:` for one with a default (area 2).
    pub absent: Option<FieldDefault>,
}

/// What an object or a variant is made of.
#[derive(Clone, Debug)]
pub(crate) struct Shape {
    /// The object's type, or the variant's enum.
    pub ty: Rc<str>,
    /// The variant, for a variant.
    pub variant: Option<Rc<str>>,
    pub fields: Vec<Field>,
}

#[derive(Debug)]
pub(crate) struct Types {
    graph: Graph,
    validators: RefCell<HashMap<TypeRef, Rc<Validator>>>,
    shapes: RefCell<HashMap<Key, Rc<Shape>>>,
}

/// The part of a dotted name after its last dot: `receipt` in
/// `PaymentOutcome.paid.receipt`.
pub(crate) fn last(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(_, last)| last)
}

/// A variant's enum and name from its dotted name: `PaymentOutcome` and
/// `paid`.
fn split(name: &str) -> (&str, &str) {
    name.rsplit_once('.').unwrap_or(("", name))
}

impl Types {
    pub fn new(graph: Graph) -> Types {
        Types {
            graph,
            validators: RefCell::new(HashMap::new()),
            shapes: RefCell::new(HashMap::new()),
        }
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    fn node(&self, key: &Key) -> Option<&Node> {
        self.graph.get(key)
    }

    /// A node's name now, as written: its key's born name when the graph
    /// has no node for it, as for the standard library's enums.
    pub fn name<'a>(&'a self, key: &'a Key) -> &'a str {
        self.node(key).map_or(key.name(), |node| node.name.as_str())
    }

    /// A field's own name, as a record holds it.
    pub fn field_name<'a>(&'a self, key: &'a Key) -> &'a str {
        last(self.name(key))
    }

    /// What `ty` is built on, its rules collected along the way, most
    /// specific first; `None` for an object, an enum or a container.
    fn chain(&self, ty: &TypeRef, rules: &mut Vec<Rules>) -> Option<Builtin> {
        // A chain is acyclic once checked; the bound only keeps a defect
        // from looping.
        let mut current = ty.clone();
        for _ in 0..256 {
            match current {
                TypeRef::Builtin(builtin) => return Some(builtin),
                TypeRef::Nullable(inner) => current = *inner,
                TypeRef::Node(key) => match &self.node(&key)?.detail {
                    Detail::Type {
                        base: Base::Refines(parent),
                        rules: own,
                        ..
                    } => {
                        rules.push(own.clone());
                        current = parent.clone();
                    }
                    _ => return None,
                },
                TypeRef::Field(key) => match &self.node(&key)?.detail {
                    Detail::Field { ty, rules: own, .. } => {
                        rules.push(own.clone());
                        current = ty.clone();
                    }
                    _ => return None,
                },
                _ => return None,
            }
        }
        None
    }

    /// What values of `ty` are made of: `Text` for an `Email`.
    pub fn representation(&self, ty: &TypeRef) -> Option<Builtin> {
        self.chain(ty, &mut Vec::new()).map(Builtin::representation)
    }

    /// Whether `ty` is a set.
    pub fn is_set(&self, ty: &TypeRef) -> bool {
        let mut current = ty;
        loop {
            match current {
                TypeRef::Set(_) => return true,
                TypeRef::Nullable(inner) => current = inner,
                TypeRef::Field(key) => match self.node(key).map(|node| &node.detail) {
                    Some(Detail::Field { ty, .. }) => current = ty,
                    _ => return false,
                },
                _ => return false,
            }
        }
    }

    /// A literal as a value of `ty`, the type it goes into (area 2,
    /// question 15): text as a UUID, an instant or a date when `ty` is
    /// built on one, a number at a decimal type's scale. `None` for a
    /// literal `ty` can't hold, which the checker rules out.
    pub fn literal(&self, literal: &Literal, ty: &TypeRef) -> Option<Value> {
        match literal {
            Literal::Bool(value) => Some(Value::Bool(*value)),
            Literal::None => Some(Value::None),
            Literal::Text(text) => match self.representation(ty) {
                Some(builtin) => crate::eval::text_literal(builtin, text),
                None => Some(Value::text(text)),
            },
            Literal::Duration(written) => jadpo_std::Dur::parse(written)
                .and_then(|dur| i64::try_from(dur.millis).ok())
                .map(Value::Duration),
            Literal::Number { value, decimal } => {
                let decimal = match self.representation(ty) {
                    Some(builtin) => builtin == Builtin::Decimal,
                    None => *decimal,
                };
                if decimal {
                    self.validator(ty)
                        .store(Value::Decimal(jadpo_value::Decimal::new(
                            value.mantissa,
                            value.scale,
                        )))
                } else if value.scale == 0 {
                    i64::try_from(value.mantissa).ok().map(Value::Int)
                } else {
                    None
                }
            }
        }
    }

    /// `ty`'s rules ready to run.
    pub fn validator(&self, ty: &TypeRef) -> Rc<Validator> {
        if let Some(found) = self.validators.borrow().get(ty) {
            return Rc::clone(found);
        }
        let mut rules = Vec::new();
        let builtin = self.chain(ty, &mut rules);
        let validator = Rc::new(Validator::new(builtin, rules));
        self.validators
            .borrow_mut()
            .insert(ty.clone(), Rc::clone(&validator));
        validator
    }

    /// The fields whose parent is `parent`, in the order they are written.
    fn fields(&self, parent: &Key) -> Vec<Field> {
        let Some(id) = self.graph.find(parent) else {
            return Vec::new();
        };
        let mut found: Vec<&Node> = self
            .graph
            .children(id)
            .map(|child| self.graph.node(child))
            .filter(|node| node.kind == Kind::Field)
            .collect();
        found.sort_by_key(|node| {
            (
                node.place.as_ref().map_or(u32::MAX, |p| p.start),
                node.key.clone(),
            )
        });
        found
            .into_iter()
            .filter_map(|node| match &node.detail {
                Detail::Field {
                    section,
                    optional,
                    default,
                    ..
                } => Some(Field {
                    key: node.key.clone(),
                    name: Rc::from(last(&node.name)),
                    section: *section,
                    absent: default
                        .clone()
                        .or_else(|| optional.then_some(FieldDefault::Literal(Literal::None))),
                }),
                _ => None,
            })
            .collect()
    }

    /// What the object type, entity or variant `key` is made of.
    pub fn shape(&self, key: &Key) -> Rc<Shape> {
        if let Some(found) = self.shapes.borrow().get(key) {
            return Rc::clone(found);
        }
        let shape = Rc::new(match key.kind() {
            Some(Kind::Variant) => {
                let (ty, variant) = split(self.name(key));
                Shape {
                    ty: Rc::from(ty),
                    variant: Some(Rc::from(variant)),
                    fields: self.fields(key),
                }
            }
            _ => Shape {
                ty: Rc::from(self.name(key)),
                variant: None,
                fields: self.fields(key),
            },
        });
        self.shapes
            .borrow_mut()
            .insert(key.clone(), Rc::clone(&shape));
        shape
    }

    /// The object or variant `key`, built from `given` fields, each by its
    /// key, in the order its type declares them; `None` when one is
    /// missing.
    pub fn build(&self, key: &Key, mut given: Vec<(Key, Value)>) -> Option<Value> {
        let shape = self.shape(key);
        let mut fields = Vec::with_capacity(shape.fields.len());
        for field in &shape.fields {
            let value = match given.iter().position(|(key, _)| *key == field.key) {
                Some(at) => given.swap_remove(at).1,
                None => match field.absent.as_ref()? {
                    FieldDefault::Literal(literal) => {
                        self.literal(literal, &TypeRef::Field(field.key.clone()))?
                    }
                    FieldDefault::Variant(variant) => self.build(variant, Vec::new())?,
                },
            };
            fields.push((Rc::clone(&field.name), value));
        }
        Some(match &shape.variant {
            Some(variant) => Value::Variant(Rc::new(jadpo_value::Variant {
                ty: Rc::clone(&shape.ty),
                name: Rc::clone(variant),
                fields,
            })),
            None => Value::Record(Rc::new(jadpo_value::Record {
                ty: Rc::clone(&shape.ty),
                fields,
            })),
        })
    }

    /// The failure `key`, with `given` fields in its client and internal
    /// sections, ready to raise; `None` when `key` isn't a declared failure
    /// or a field is missing.
    pub fn failure(&self, key: &Key, mut given: Vec<(Key, Value)>) -> Option<Failure> {
        let node = self.node(key)?;
        let Detail::Type {
            failure: Some(detail),
            ..
        } = &node.detail
        else {
            return None;
        };
        let mut failure = Failure::new(key.as_str(), node.name.as_str(), detail.code.as_str());
        failure.kind = detail.kind.clone();
        failure.status = detail.status;
        failure.message = detail.message.clone();
        for field in self.fields(key) {
            let at = given.iter().position(|(key, _)| *key == field.key)?;
            let (_, value) = given.swap_remove(at);
            let entry = (field.name.to_string(), value);
            match field.section {
                Some(Section::Client) => failure.client.push(entry),
                _ => failure.internal.push(entry),
            }
        }
        Some(failure)
    }
}
