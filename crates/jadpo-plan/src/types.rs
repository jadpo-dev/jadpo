//! Type plans (docs/compiler-runtime.md §2.1 and §2.5): what a value of
//! each type is made of, read from the graph once so both executors build,
//! check and read values the same way. The plans are `jadpo-std`'s plain
//! data ([`jadpo_std::types`]); this numbers them and answers questions
//! about a type's chain.
//!
//! - A [`ValidatorPlan`] is a type's rules along its chain, most specific
//!   first, and the built-in type at the bottom: what the runtime's
//!   validator checks a value against and the scale it keeps a decimal at.
//! - A [`ShapePlan`] is an object's, entity's or variant's fields in the
//!   order the type declares them (chunk 2.7 recorded it in the graph),
//!   each with what an `Input` left without it holds; a variant's also
//!   says where it stands among its enum's variants, which is what sets
//!   order variants by (area 2, "Set order").
//! - A [`FailurePlan`] is a declared failure's code, kind, status, message
//!   and fields, each in its client or internal section (area 4).
//!
//! Plans for what bodies use are numbered once and looked up by id; the
//! queries that walk a type's chain ([`Types::chain`],
//! [`Types::collection`] and the rest) answer for any type, for tooling
//! such as the probe values chunk 2.7 runs callables with.

use std::collections::HashMap;
use std::sync::Arc;

use jadpo_graph::body::Literal;
use jadpo_graph::{Base, Detail, FieldDefault, Graph, Key, Kind, Node, Rules, TypeRef};
use jadpo_std::Builtin;

use crate::constant::{self, Constant};

pub use jadpo_std::types::{
    FailureId, FailurePlan, FieldPlan, ShapeId, ShapePlan, ValidatorId, ValidatorPlan, VariantPlan,
};

/// The part of a dotted name after its last dot: `receipt` in
/// `PaymentOutcome.paid.receipt`.
pub fn last(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(_, last)| last)
}

/// A variant's enum and name from its dotted name: `PaymentOutcome` and
/// `paid`.
fn split(name: &str) -> (&str, &str) {
    name.rsplit_once('.').unwrap_or(("", name))
}

/// A chain is acyclic once checked; the bound only keeps a defect from
/// looping.
const CHAIN: usize = 256;

/// Every type plan a project's bodies use, and the queries over its graph.
#[derive(Debug)]
pub struct Types {
    graph: Arc<Graph>,
    validators: Vec<ValidatorPlan>,
    validator_ids: HashMap<TypeRef, ValidatorId>,
    shapes: Vec<ShapePlan>,
    shape_ids: HashMap<Key, ShapeId>,
    failures: Vec<FailurePlan>,
    failure_ids: HashMap<Key, FailureId>,
}

impl Types {
    pub(crate) fn new(graph: Arc<Graph>) -> Types {
        Types {
            graph,
            validators: Vec::new(),
            validator_ids: HashMap::new(),
            shapes: Vec::new(),
            shape_ids: HashMap::new(),
            failures: Vec::new(),
            failure_ids: HashMap::new(),
        }
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    fn node(&self, key: &Key) -> Option<&Node> {
        self.graph.get(key)
    }

    // --- numbered plans ---------------------------------------------------

    pub fn validator(&self, id: ValidatorId) -> Option<&ValidatorPlan> {
        self.validators.get(id.0 as usize)
    }

    /// Every validator plan, by number.
    pub fn validators(&self) -> &[ValidatorPlan] {
        &self.validators
    }

    pub fn shape(&self, id: ShapeId) -> Option<&ShapePlan> {
        self.shapes.get(id.0 as usize)
    }

    /// Every shape plan, by number.
    pub fn shapes(&self) -> &[ShapePlan] {
        &self.shapes
    }

    pub fn failure(&self, id: FailureId) -> Option<&FailurePlan> {
        self.failures.get(id.0 as usize)
    }

    /// Every failure plan, by number: what the runtime raises a missing
    /// record's failure from.
    pub fn failures(&self) -> &[FailurePlan] {
        &self.failures
    }

    /// The number of `ty`'s validator, numbering it on first use.
    pub(crate) fn validator_id(&mut self, ty: &TypeRef) -> ValidatorId {
        if let Some(id) = self.validator_ids.get(ty) {
            return *id;
        }
        let plan = self.validator_plan(ty);
        let id = ValidatorId(self.validators.len() as u32);
        self.validators.push(plan);
        self.validator_ids.insert(ty.clone(), id);
        id
    }

    /// The number of `key`'s shape, numbering it on first use.
    pub(crate) fn shape_id(&mut self, key: &Key) -> Option<ShapeId> {
        if let Some(id) = self.shape_ids.get(key) {
            return Some(*id);
        }
        let mut plan = self.shape_plan(key)?;
        for field in &mut plan.fields {
            field.validator = self.validator_id(&TypeRef::Field(Key::from_text(&field.key)));
        }
        let id = ShapeId(self.shapes.len() as u32);
        self.shapes.push(plan);
        self.shape_ids.insert(key.clone(), id);
        Some(id)
    }

    /// The number of failure `key`'s plan, numbering it on first use.
    pub(crate) fn failure_id(&mut self, key: &Key) -> Option<FailureId> {
        if let Some(id) = self.failure_ids.get(key) {
            return Some(*id);
        }
        let mut plan = self.failure_plan(key)?;
        for field in &mut plan.fields {
            field.validator = self.validator_id(&TypeRef::Field(Key::from_text(&field.key)));
        }
        let id = FailureId(self.failures.len() as u32);
        self.failures.push(plan);
        self.failure_ids.insert(key.clone(), id);
        Some(id)
    }

    // --- names ------------------------------------------------------------

    /// A node's name now, as written: its key's born name when the graph
    /// has no node for it, as for the standard library's enums.
    pub fn name<'a>(&'a self, key: &'a Key) -> &'a str {
        self.node(key).map_or(key.name(), |node| node.name.as_str())
    }

    /// A field's own name, as a value holds it.
    pub fn field_name<'a>(&'a self, key: &'a Key) -> &'a str {
        last(self.name(key))
    }

    /// The type or entity called `name` now.
    pub fn named(&self, name: &str) -> Option<Key> {
        [Kind::Type, Kind::Entity]
            .into_iter()
            .find_map(|kind| self.graph.named(kind, name))
            .map(|id| self.graph.node(id).key.clone())
    }

    /// Variant `variant` of enum `ty`, by their names now; the standard
    /// library's enums have no node, so their key is made from the names.
    pub fn variant_named(&self, ty: &str, variant: &str) -> Key {
        let dotted = format!("{ty}.{variant}");
        self.graph.named(Kind::Variant, &dotted).map_or_else(
            || Key::new(Kind::Variant, &dotted),
            |id| self.graph.node(id).key.clone(),
        )
    }

    // --- queries over any type ----------------------------------------------

    /// What `ty` is built on, its rules collected along the way, most
    /// specific first; `None` for an object, an enum or a container.
    pub fn chain(&self, ty: &TypeRef) -> Option<(Builtin, Vec<Rules>)> {
        let mut rules = Vec::new();
        let mut current = ty.clone();
        for _ in 0..CHAIN {
            match current {
                TypeRef::Builtin(builtin) => {
                    // A built-in type's own rules, such as `PageSize`'s range.
                    rules.extend(builtin.rules().cloned());
                    return Some((builtin, rules));
                }
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

    /// `ty`'s rules along its chain, as a validator checks them. A chain
    /// that reaches no built-in type (an object, an enum, a collection)
    /// still keeps the rules on the way, such as a list type's lengths.
    pub fn validator_plan(&self, ty: &TypeRef) -> ValidatorPlan {
        match self.chain(ty) {
            Some((builtin, rules)) => ValidatorPlan {
                builtin: Some(builtin),
                rules,
            },
            None => ValidatorPlan {
                builtin: None,
                rules: self.length_rules(ty),
            },
        }
    }

    /// The rules along `ty`'s chain down to a collection, most specific
    /// first: the lengths a list or set type adds.
    pub fn length_rules(&self, ty: &TypeRef) -> Vec<Rules> {
        let mut rules = Vec::new();
        let mut current = ty.clone();
        for _ in 0..CHAIN {
            match current {
                TypeRef::Nullable(inner) => current = *inner,
                TypeRef::Node(key) => match self.node(&key).map(|node| &node.detail) {
                    Some(Detail::Type {
                        base: Base::Refines(parent),
                        rules: own,
                        ..
                    }) => {
                        rules.push(own.clone());
                        current = parent.clone();
                    }
                    _ => break,
                },
                TypeRef::Field(key) => match self.node(&key).map(|node| &node.detail) {
                    Some(Detail::Field { ty, rules: own, .. }) => {
                        rules.push(own.clone());
                        current = ty.clone();
                    }
                    _ => break,
                },
                _ => break,
            }
        }
        rules
    }

    /// Whether a value of `ty` may be `none`: a `T?`, or a field type
    /// declared as one.
    pub fn is_nullable(&self, ty: &TypeRef) -> bool {
        let mut current = ty;
        for _ in 0..CHAIN {
            match current {
                TypeRef::Nullable(_) => return true,
                TypeRef::Field(key) => match self.node(key).map(|node| &node.detail) {
                    Some(Detail::Field { ty, .. }) => current = ty,
                    _ => return false,
                },
                _ => return false,
            }
        }
        false
    }

    /// What values of `ty` are made of: `Text` for an `Email`.
    pub fn representation(&self, ty: &TypeRef) -> Option<Builtin> {
        self.chain(ty).map(|(builtin, _)| builtin.representation())
    }

    /// The `List<T>` or `Set<T>` that `ty` is, or that the named type or
    /// field type `ty` is built on.
    pub fn collection<'t>(&'t self, ty: &'t TypeRef) -> Option<&'t TypeRef> {
        let mut current = ty;
        for _ in 0..CHAIN {
            match current {
                TypeRef::List(_) | TypeRef::Set(_) => return Some(current),
                TypeRef::Nullable(inner) => current = inner,
                TypeRef::Field(key) => match self.node(key).map(|node| &node.detail) {
                    Some(Detail::Field { ty, .. }) => current = ty,
                    _ => return None,
                },
                TypeRef::Node(key) => match self.node(key).map(|node| &node.detail) {
                    Some(Detail::Type {
                        base: Base::Refines(parent),
                        ..
                    }) => current = parent,
                    _ => return None,
                },
                _ => return None,
            }
        }
        None
    }

    /// Whether `ty` is a set: written `Set<T>`, or a named type or a field
    /// type built on one.
    pub fn is_set(&self, ty: &TypeRef) -> bool {
        matches!(self.collection(ty), Some(TypeRef::Set(_)))
    }

    /// A literal as a value of `ty`, the type it goes into (area 2,
    /// question 15); `None` for a literal `ty` can't hold, which the
    /// checker rules out.
    pub fn literal(&self, literal: &Literal, ty: &TypeRef) -> Option<Constant> {
        let chain = self.chain(ty);
        let builtin = chain.as_ref().map(|(builtin, _)| *builtin);
        let scale = chain
            .as_ref()
            .and_then(|(_, rules)| rules.iter().find_map(|rules| rules.scale));
        constant::literal(literal, builtin, scale)
    }

    /// `text` as a literal of `ty` holds it, when `ty` is built on a
    /// built-in type: a UUID, an instant or a date from its ISO text, text
    /// otherwise.
    pub fn text(&self, ty: &TypeRef, text: &str) -> Option<Constant> {
        let builtin = self.representation(ty)?;
        constant::text(Some(builtin), text)
    }

    /// The fields whose parent is `parent`, in the order they are declared,
    /// copied fields where the line that copied them stands.
    fn fields(&self, parent: &Key, depth: usize) -> Vec<FieldPlan> {
        let Some(id) = self.graph.find(parent) else {
            return Vec::new();
        };
        self.graph
            .fields(id)
            .into_iter()
            .map(|child| self.graph.node(child))
            .filter_map(|node| match &node.detail {
                Detail::Field {
                    section,
                    optional,
                    default,
                    ..
                } => {
                    let absent = match default {
                        Some(FieldDefault::Literal(literal)) => {
                            self.literal(literal, &TypeRef::Field(node.key.clone()))
                        }
                        Some(FieldDefault::Variant(variant)) => {
                            self.variant_constant(variant, depth + 1)
                        }
                        None if *optional => Some(Constant::None),
                        None => None,
                    };
                    let omittable = *optional && default.is_none();
                    let none_left_out =
                        omittable && !self.is_nullable(&TypeRef::Field(node.key.clone()));
                    Some(FieldPlan {
                        key: node.key.as_str().to_string(),
                        name: last(&node.name).to_string(),
                        validator: ValidatorId(u32::MAX),
                        absent,
                        omittable,
                        none_left_out,
                        section: *section,
                    })
                }
                _ => None,
            })
            .collect()
    }

    /// Variant `key` built with nothing given, as a field's default: every
    /// field it carries takes its own default; `None` when one has none.
    fn variant_constant(&self, key: &Key, depth: usize) -> Option<Constant> {
        if depth > CHAIN {
            return None;
        }
        let shape = self.shape_with(key, depth)?;
        let variant = shape.variant?;
        let fields = shape
            .fields
            .into_iter()
            .map(|field| Some((field.name, field.absent?)))
            .collect::<Option<Vec<_>>>()?;
        Some(Constant::Variant {
            ty: shape.ty,
            ordinal: variant.ordinal,
            name: variant.name,
            fields,
        })
    }

    /// What the object type, entity or variant `key` is made of; `None`
    /// for anything else. Its fields' validators aren't numbered here.
    pub fn shape_plan(&self, key: &Key) -> Option<ShapePlan> {
        self.shape_with(key, 0)
    }

    fn shape_with(&self, key: &Key, depth: usize) -> Option<ShapePlan> {
        match key.kind()? {
            Kind::Variant => {
                let (ty, variant) = split(self.name(key));
                let ordinal = match self.node(key).map(|node| &node.detail) {
                    Some(Detail::Variant { order, .. }) => *order,
                    // The standard library's enums have no node.
                    _ => jadpo_std::variant_ordinal(ty, variant)?,
                };
                Some(ShapePlan {
                    ty: ty.to_string(),
                    variant: Some(VariantPlan {
                        name: variant.to_string(),
                        ordinal,
                    }),
                    fields: self.fields(key, depth),
                })
            }
            Kind::Type | Kind::Entity => Some(ShapePlan {
                ty: self.name(key).to_string(),
                variant: None,
                fields: self.fields(key, depth),
            }),
            _ => None,
        }
    }

    /// The declared failure `key`; `None` when it isn't one.
    pub fn failure_plan(&self, key: &Key) -> Option<FailurePlan> {
        let node = self.node(key)?;
        let Detail::Type {
            failure: Some(detail),
            ..
        } = &node.detail
        else {
            return None;
        };
        Some(FailurePlan {
            key: key.as_str().to_string(),
            name: node.name.clone(),
            code: detail.code.clone(),
            kind: detail.kind.clone(),
            status: detail.status,
            message: detail.message.clone(),
            fields: self.fields(key, 0),
        })
    }
}
