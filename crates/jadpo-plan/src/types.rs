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
    /// Each rule across an object's fields, by key, with its typed body
    /// (area 2, "Rules across fields").
    rule_bodies: HashMap<Key, Arc<jadpo_graph::body::Body>>,
    validators: Vec<ValidatorPlan>,
    validator_ids: HashMap<TypeRef, ValidatorId>,
    shapes: Vec<ShapePlan>,
    shape_ids: HashMap<Key, ShapeId>,
    failures: Vec<FailurePlan>,
    failure_ids: HashMap<Key, FailureId>,
    /// The folders of the provider packages, each with its `/`: their
    /// types are read from JSON open to members they don't declare (area
    /// 11, chunk 7.2c).
    providers: Vec<String>,
    /// The sign-in providers `SignInMethod` has a value for after its own,
    /// in order (area 8, "Providers"; chunk A7b).
    sign_in_providers: Vec<String>,
}

impl Types {
    pub(crate) fn new(
        graph: Arc<Graph>,
        rule_bodies: HashMap<Key, Arc<jadpo_graph::body::Body>>,
    ) -> Types {
        Types {
            graph,
            rule_bodies,
            validators: Vec::new(),
            validator_ids: HashMap::new(),
            shapes: Vec::new(),
            shape_ids: HashMap::new(),
            failures: Vec::new(),
            failure_ids: HashMap::new(),
            providers: Vec::new(),
            sign_in_providers: Vec::new(),
        }
    }

    /// The same, with the provider packages' folders (area 11, chunk
    /// 7.2c).
    /// `SignInMethod`'s values after the language's own: one for each
    /// sign-in provider, in order (chunk A7b).
    pub(crate) fn with_sign_in_providers(mut self, names: Vec<String>) -> Types {
        self.sign_in_providers = names;
        self
    }

    /// The sign-in providers `SignInMethod` has values for, after its
    /// own.
    pub(crate) fn sign_in_providers(&self) -> &[String] {
        &self.sign_in_providers
    }

    pub(crate) fn with_providers(mut self, folders: Vec<String>) -> Types {
        self.providers = folders
            .into_iter()
            .map(|folder| format!("{}/", folder.trim_end_matches('/')))
            .collect();
        self
    }

    /// Whether `key`'s declaration is a provider package's.
    fn is_providers(&self, key: &Key) -> bool {
        self.node(key)
            .and_then(|node| node.place.as_ref())
            .is_some_and(|place| {
                self.providers
                    .iter()
                    .any(|folder| place.file.starts_with(folder.as_str()))
            })
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    fn node(&self, key: &Key) -> Option<&Node> {
        self.graph.get(key)
    }

    // --- numbered plans ---------------------------------------------------
    //
    // Numbered while the plan is built; once it is, they belong to the
    // runtime's plan (`Plan::runtime`), which every reader reads them from
    // (chunk 8.1a).

    pub(crate) fn shape(&self, id: ShapeId) -> Option<&ShapePlan> {
        self.shapes.get(id.0 as usize)
    }

    pub(crate) fn failure(&self, id: FailureId) -> Option<&FailurePlan> {
        self.failures.get(id.0 as usize)
    }

    /// The validator, shape and failure plans numbered so far, handed to
    /// the runtime's plan as the plan is finished, leaving none here.
    pub(crate) fn take_tables(&mut self) -> (Vec<ValidatorPlan>, Vec<ShapePlan>, Vec<FailurePlan>) {
        (
            std::mem::take(&mut self.validators),
            std::mem::take(&mut self.shapes),
            std::mem::take(&mut self.failures),
        )
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

    /// The number of the built-in `IssuedToken`'s or `IssuedKey`'s shape
    /// (area 8), or `DecodeProblem`'s (area 19), numbering it on first
    /// use. None is in the graph, so the shape is written from the fields
    /// the standard library lists.
    pub(crate) fn issued_shape(&mut self, name: &str) -> ShapeId {
        self.fixed_shape(
            name,
            jadpo_std::http::builtin_object_fields(name).unwrap_or_default(),
        )
    }

    /// The shape of a type the language declares with fixed fields, each
    /// built on a built-in type: an issued token or key (area 8), or a
    /// standard object type from its data entry (area 19), numbering it
    /// on first use.
    pub(crate) fn fixed_shape(
        &mut self,
        name: &str,
        fields: &[(&str, jadpo_std::Builtin)],
    ) -> ShapeId {
        let fields: Vec<(&str, TypeRef)> = fields
            .iter()
            .map(|(field, builtin)| (*field, TypeRef::Builtin(*builtin)))
            .collect();
        self.typed_shape(name, &fields)
    }

    /// The shape of a batch type (area 12, chunk 6.4), or a stored `File`
    /// (chunk 6.4b), each field typed by its part.
    pub(crate) fn batch_shape(&mut self, name: &str) -> ShapeId {
        let fields: Vec<(&str, TypeRef)> = jadpo_std::batch::kept_fields(name)
            .unwrap_or_default()
            .iter()
            .map(|(field, part)| (*field, batch_part(*part)))
            .collect();
        self.typed_shape(name, &fields)
    }

    /// The shape of a standard object type from its data entry's parts
    /// (area 19): each field the type its part is built on, or the
    /// standard type it holds.
    pub(crate) fn standard_shape(
        &mut self,
        name: &str,
        fields: &[(&str, jadpo_std::Part)],
    ) -> ShapeId {
        let typed: Vec<(&str, TypeRef)> = fields
            .iter()
            .map(|(field, part)| (*field, part_type(name, field, *part)))
            .collect();
        let id = self.typed_shape(name, &typed);
        // Sent as RFC 9457 sends a problem (chunk 5.5n): a part that may
        // be `none` may be left out, and reads as `none` when it is.
        if jadpo_std::standard_type(name)
            .is_some_and(|entry| matches!(entry.json, jadpo_std::standard::Json::Members(_)))
            && let Some(shape) = self.shapes.get_mut(id.0 as usize)
        {
            for (plan, (_, part)) in shape.fields.iter_mut().zip(fields) {
                if part.required().1 {
                    plan.absent = Some(Constant::None);
                }
            }
        }
        id
    }

    pub(crate) fn typed_shape(&mut self, name: &str, fields: &[(&str, TypeRef)]) -> ShapeId {
        let key = Key::new(Kind::Type, name);
        if let Some(id) = self.shape_ids.get(&key) {
            return *id;
        }
        let fields = fields
            .iter()
            .map(|(field, ty)| FieldPlan {
                key: format!("field {name}.{field}"),
                name: (*field).to_string(),
                validator: self.validator_id(ty),
                absent: None,
                omittable: false,
                none_left_out: false,
                section: None,
                wire: None,
            })
            .collect();
        let id = ShapeId(self.shapes.len() as u32);
        self.shapes.push(ShapePlan {
            ty: name.to_string(),
            open: false,
            variant: None,
            fields,
        });
        self.shape_ids.insert(key, id);
        id
    }

    /// The shape of the standard object type `key` names (`InstantRange`,
    /// area 19), which has no declaration in the graph: its data entry's
    /// fields. `None` for anything else.
    pub(crate) fn standard_object_shape(&mut self, key: &Key) -> Option<ShapeId> {
        if key.kind() != Some(Kind::Type) || self.node(key).is_some() {
            return None;
        }
        // `FirstSignIn` or a sign-in event a test builds (chunk 7.3b4),
        // checked as each field's type where it is built.
        if let Some(fields) = jadpo_std::http::test_built_fields(key.name()) {
            let id = self.generic_shape(key.name(), fields);
            // A magic-link route's input leaves out what may be `none`, as
            // a request's body may; the checker requires the rest.
            if jadpo_std::http::magic_link_input(key.name())
                && let Some(shape) = self.shapes.get_mut(id.0 as usize)
            {
                for field in &mut shape.fields {
                    field.absent = Some(Constant::None);
                }
            }
            // A `TestAccount` leaves out its company and strength (chunk
            // A10), `none` when it does.
            if key.name() == jadpo_std::http::TEST_ACCOUNT
                && let Some(shape) = self.shapes.get_mut(id.0 as usize)
            {
                for field in &mut shape.fields {
                    if jadpo_std::http::TEST_ACCOUNT_OPTIONAL.contains(&&*field.name) {
                        field.absent = Some(Constant::None);
                    }
                }
            }
            return Some(id);
        }
        let entry = jadpo_std::standard_type(key.name())?;
        match entry.holds {
            jadpo_std::standard::Holds::Object(fields) => {
                Some(self.standard_shape(entry.name, fields))
            }
            jadpo_std::standard::Holds::Range => Some(self.range_shape()),
            jadpo_std::standard::Holds::Feature => {
                Some(self.generic_shape(entry.name, &jadpo_std::standard::geo::FEATURE_FIELDS))
            }
            jadpo_std::standard::Holds::FeatureCollection => Some(self.generic_shape(
                entry.name,
                &jadpo_std::standard::geo::FEATURE_COLLECTION_FIELDS,
            )),
            _ => None,
        }
    }

    /// The shape of a feature or a feature collection (area 20, chunk
    /// 5.5u6): its fields by name. One shape serves every pair of type
    /// arguments, each field checked as its argument's type where it is
    /// built or read, so the shape adds no rule of its own.
    pub(crate) fn generic_shape(&mut self, name: &str, fields: &[&str]) -> ShapeId {
        let key = Key::new(Kind::Type, name);
        if let Some(id) = self.shape_ids.get(&key) {
            return *id;
        }
        let validator = self.validator_id(&TypeRef::Std(name.to_string(), Vec::new()));
        let fields = fields
            .iter()
            .map(|field| FieldPlan {
                key: format!("field {name}.{field}"),
                name: (*field).to_string(),
                validator,
                absent: None,
                omittable: false,
                none_left_out: false,
                section: None,
                wire: None,
            })
            .collect();
        let id = ShapeId(self.shapes.len() as u32);
        self.shapes.push(ShapePlan {
            ty: name.to_string(),
            open: false,
            variant: None,
            fields,
        });
        self.shape_ids.insert(key, id);
        id
    }

    /// The shape of `Range<T>` (area 19): its `start` and `end`, each a
    /// `T` or `none`. One shape serves every `T`: an end is checked as its
    /// `T` where it is built or read (its codec), so the shape adds no
    /// rule of its own.
    pub(crate) fn range_shape(&mut self) -> ShapeId {
        let key = Key::new(Kind::Type, jadpo_std::RANGE);
        if let Some(id) = self.shape_ids.get(&key) {
            return *id;
        }
        let validator = self.validator_id(&TypeRef::Std(jadpo_std::RANGE.to_string(), Vec::new()));
        let fields = jadpo_std::RANGE_FIELDS
            .iter()
            .map(|field| FieldPlan {
                key: format!("field {}.{field}", jadpo_std::RANGE),
                name: (*field).to_string(),
                validator,
                absent: None,
                omittable: false,
                none_left_out: false,
                section: None,
                wire: None,
            })
            .collect();
        let id = ShapeId(self.shapes.len() as u32);
        self.shapes.push(ShapePlan {
            ty: jadpo_std::RANGE.to_string(),
            open: false,
            variant: None,
            fields,
        });
        self.shape_ids.insert(key, id);
        id
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

    /// How the provider package's field `key` is sent, when its `wire:`
    /// or its `decode:` and `encode:` say (area 11, "Decoding"; chunk
    /// 7.2c2).
    pub fn field_coding(&self, key: &Key) -> Option<&jadpo_graph::FieldCoding> {
        match &self.node(key)?.detail {
            Detail::Field { coding, .. } => coding.as_ref(),
            _ => None,
        }
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
                // A standard type held as text (area 19, chunk 5.5i) has no
                // node: it is text with its own check, or, held as a whole
                // number, an `Int` with its range (chunk 5.5n), or its
                // built-in type with its bounds (chunk 5.5i3).
                TypeRef::Std(name, args) if args.is_empty() => {
                    let (builtin, own) = jadpo_std::standard::declared(&name)?;
                    rules.push(own);
                    current = TypeRef::Builtin(builtin);
                }
                // A standard object type's field (area 19) has no node: its
                // data entry's part says what it is built on and the range
                // a whole number keeps, which `LocalTime.hour(...)` checks.
                TypeRef::Field(key) if self.node(&key).is_none() => {
                    let (builtin, own) = standard_part(&key)?;
                    rules.push(own);
                    current = TypeRef::Builtin(builtin);
                }
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
                across: Vec::new(),
            },
            None => ValidatorPlan {
                builtin: None,
                rules: self.length_rules(ty),
                across: self.across(ty),
            },
        }
    }

    /// The rules across the fields of the object type `ty` names, in the
    /// order written, each lowered from its typed body (area 2, "Rules
    /// across fields"), or a standard object type's rules across its parts,
    /// each run by its check in `jadpo-std` (area 19, chunk 5.5t2), through
    /// a field or a type built on it; none for any other type.
    pub fn across(&self, ty: &TypeRef) -> Vec<jadpo_std::across::CrossRule> {
        let mut current = ty.clone();
        for _ in 0..CHAIN {
            match current {
                TypeRef::Nullable(inner) => current = *inner,
                TypeRef::Std(name, _) => return jadpo_std::standard::across::cross_rules(&name),
                TypeRef::Field(key) => match self.node(&key).map(|node| &node.detail) {
                    Some(Detail::Field { ty, .. }) => current = ty.clone(),
                    _ => return Vec::new(),
                },
                TypeRef::Node(key) => match self.node(&key).map(|node| &node.detail) {
                    Some(Detail::Type {
                        base: Base::Refines(parent),
                        ..
                    }) => current = parent.clone(),
                    _ => return self.declared_across(&key),
                },
                _ => return Vec::new(),
            }
        }
        Vec::new()
    }

    /// The rules the object type `key` names across its fields.
    fn declared_across(&self, key: &Key) -> Vec<jadpo_std::across::CrossRule> {
        let Some(id) = self.graph.find(key) else {
            return Vec::new();
        };
        let mut rules: Vec<(u32, &Node)> = self
            .graph
            .children(id)
            .map(|child| self.graph.node(child))
            .filter_map(|node| match &node.detail {
                Detail::Rule { order, .. } => Some((*order, node)),
                _ => None,
            })
            .collect();
        rules.sort_by_key(|(order, _)| *order);
        rules
            .into_iter()
            .filter_map(|(_, node)| {
                let body = self.rule_bodies.get(&node.key)?;
                let jadpo_graph::body::Node::Stmt(jadpo_graph::body::Stmt::Return {
                    value: Some(value),
                }) = &body.site(*body.block.first()?).node
                else {
                    return None;
                };
                Some(jadpo_std::across::CrossRule {
                    name: last(&node.name).to_string(),
                    locals: u32::try_from(body.locals.len()).ok()?,
                    check: self.rule_expr(body, *value)?,
                })
            })
            .collect()
    }

    /// A rule's condition, or a part of it, as the runtime runs it.
    fn rule_expr(
        &self,
        body: &jadpo_graph::body::Body,
        at: jadpo_graph::body::SiteId,
    ) -> Option<jadpo_std::across::RuleExpr> {
        use jadpo_graph::body::{ArmBody, BinaryOp, Callee, Expr, Node as Site, Pattern, UnaryOp};
        use jadpo_std::across::{RuleArm, RuleExpr, RuleOp, RulePattern};
        let site = body.site(at);
        let Site::Expr(expr) = &site.node else {
            return None;
        };
        let boxed = |at| self.rule_expr(body, at).map(Box::new);
        Some(match expr {
            Expr::Literal(literal) => RuleExpr::Constant(self.literal(literal, &site.ty)?),
            Expr::Local(local) => RuleExpr::Local(local.0),
            Expr::Field { receiver, field } => RuleExpr::Field {
                receiver: boxed(*receiver)?,
                name: self.field_name(field).to_string(),
            },
            Expr::List { items } => RuleExpr::List(
                items
                    .iter()
                    .map(|item| self.rule_expr(body, *item))
                    .collect::<Option<Vec<_>>>()?,
            ),
            Expr::Binary { op, lhs, rhs } => {
                let rounding = self
                    .chain(&body.site(*lhs).ty)
                    .filter(|(builtin, _)| *builtin == Builtin::Money)
                    .and_then(|(_, rules)| rules.iter().find_map(|rules| rules.rounding))
                    .unwrap_or(jadpo_std::Rounding::HalfEven);
                RuleExpr::Binary {
                    op: match op {
                        BinaryOp::Or => RuleOp::Or,
                        BinaryOp::And => RuleOp::And,
                        BinaryOp::Equal => RuleOp::Equal,
                        BinaryOp::NotEqual => RuleOp::NotEqual,
                        BinaryOp::In => RuleOp::In,
                        BinaryOp::Less => RuleOp::Less,
                        BinaryOp::LessEqual => RuleOp::LessEqual,
                        BinaryOp::Greater => RuleOp::Greater,
                        BinaryOp::GreaterEqual => RuleOp::GreaterEqual,
                        BinaryOp::Add => RuleOp::Add,
                        BinaryOp::Subtract => RuleOp::Subtract,
                        BinaryOp::Multiply => RuleOp::Multiply,
                        BinaryOp::Divide => RuleOp::Divide,
                        BinaryOp::Remainder => RuleOp::Remainder,
                        BinaryOp::Matches => return None,
                    },
                    lhs: boxed(*lhs)?,
                    rhs: boxed(*rhs)?,
                    rounding,
                }
            }
            Expr::Unary {
                op: UnaryOp::Not,
                operand,
            } => RuleExpr::Not(boxed(*operand)?),
            Expr::Unary {
                op: UnaryOp::Negate,
                operand,
            } => RuleExpr::Negate(boxed(*operand)?),
            Expr::Call {
                callee: Callee::Std(op),
                args,
                ..
            } => {
                // As the interpreter gives them: an optional parameter left
                // out is `none`.
                let mut given = Vec::new();
                for (param, arg) in op.operation().params.iter().zip(args) {
                    match arg {
                        Some(arg) => given.push(self.rule_expr(body, *arg)?),
                        None if param.optional => {
                            given.push(RuleExpr::Constant(Constant::None));
                        }
                        None => return None,
                    }
                }
                RuleExpr::Std {
                    op: *op,
                    args: given,
                }
            }
            Expr::Build {
                callee: Callee::Variant(key),
                fields,
                ..
            } if fields.is_empty() => {
                let shape = self.shape_plan(key)?;
                let variant = shape.variant?;
                RuleExpr::Constant(Constant::Variant {
                    ty: shape.ty,
                    ordinal: variant.ordinal,
                    name: variant.name,
                    fields: Vec::new(),
                })
            }
            Expr::Match {
                subject: Some(subject),
                outcome: false,
                arms,
            } => RuleExpr::Match {
                subject: boxed(*subject)?,
                arms: arms
                    .iter()
                    .map(|arm| {
                        let ArmBody::Value(value) = &arm.body else {
                            return None;
                        };
                        Some(RuleArm {
                            pattern: match &arm.pattern {
                                Pattern::Some => RulePattern::Some,
                                Pattern::None => RulePattern::None,
                                Pattern::True => RulePattern::True,
                                Pattern::False => RulePattern::False,
                                Pattern::Wildcard => RulePattern::Wildcard,
                                Pattern::Variant(key) => {
                                    RulePattern::Variant(self.shape_plan(key)?.variant?.ordinal)
                                }
                                Pattern::Literal(literal) => RulePattern::Literal(
                                    self.literal(literal, &body.site(*subject).ty)?,
                                ),
                                _ => return None,
                            },
                            binding: arm.binding.map(|local| local.0),
                            body: self.rule_expr(body, *value)?,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?,
            },
            _ => return None,
        })
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
                // A `Vector`'s own lengths, and its items kept as floats
                // (chunk 5.5s2).
                TypeRef::Std(name, _) => {
                    rules.extend(jadpo_std::standard::collection_rules(&name));
                    break;
                }
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
        self.container(ty)
            .filter(|found| matches!(found, TypeRef::List(_) | TypeRef::Set(_)))
    }

    /// The `List<T>`, `Set<T>` or `Map<K, V>` that `ty` is, or that the
    /// named type or field type `ty` is built on.
    fn container<'t>(&'t self, ty: &'t TypeRef) -> Option<&'t TypeRef> {
        let mut current = ty;
        for _ in 0..CHAIN {
            match current {
                TypeRef::List(_) | TypeRef::Set(_) | TypeRef::Map(..) => return Some(current),
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

    /// Whether `ty` is a map: written `Map<K, V>`, or a named type or a
    /// field type built on one.
    pub fn is_map(&self, ty: &TypeRef) -> bool {
        matches!(self.container(ty), Some(TypeRef::Map(..)))
    }

    /// A literal as a value of `ty`, the type it goes into (area 2,
    /// question 15); `None` for a literal `ty` can't hold, which the
    /// checker rules out.
    pub fn literal(&self, literal: &Literal, ty: &TypeRef) -> Option<Constant> {
        // Text where a standard object type goes is that type's canonical
        // text, read through its contract where it becomes a value.
        if let (Literal::Text(written), Some(standard)) = (literal, self.standard_object(ty)) {
            return Some(Constant::Standard {
                ty: standard,
                text: written.clone(),
            });
        }
        let chain = self.chain(ty);
        let builtin = chain.as_ref().map(|(builtin, _)| *builtin);
        let scale = chain
            .as_ref()
            .and_then(|(_, rules)| rules.iter().find_map(|rules| rules.scale));
        let decoded = constant::literal(literal, builtin, scale)?;
        Some(Self::kept_secret(chain.as_ref(), decoded))
    }

    /// The standard object type `ty` is, or is built on: `SemanticVersion`,
    /// `CardExpiry`, a `Colour` (area 19), whose values are records read
    /// from their canonical text.
    fn standard_object(&self, ty: &TypeRef) -> Option<&'static str> {
        let mut current = ty;
        for _ in 0..CHAIN {
            current = match current {
                TypeRef::Nullable(inner) => inner,
                TypeRef::Node(key) => match &self.node(key)?.detail {
                    Detail::Type {
                        base: Base::Refines(parent),
                        ..
                    } => parent,
                    _ => return None,
                },
                TypeRef::Field(key) => match &self.node(key)?.detail {
                    Detail::Field { ty, .. } => ty,
                    _ => return None,
                },
                TypeRef::Std(name, args) if args.is_empty() => {
                    let entry = jadpo_std::standard::standard_type(name)?;
                    return matches!(entry.holds, jadpo_std::standard::Holds::Object(_))
                        .then_some(entry.name);
                }
                _ => return None,
            };
        }
        None
    }

    /// `decoded` held as a secret value when the type is built on one (area
    /// 19, chunk 5.5r2), so a literal `Password` is never printed either.
    fn kept_secret(chain: Option<&(Builtin, Vec<Rules>)>, decoded: Constant) -> Constant {
        let secret = chain.and_then(|(_, rules)| {
            rules.iter().find_map(|rules| {
                rules
                    .standard
                    .and_then(jadpo_std::standard::security::secret_value)
            })
        });
        match (secret, decoded) {
            (Some(kind), Constant::Text(text)) => Constant::Secret {
                kind,
                text: jadpo_std::Hidden(text),
            },
            (_, decoded) => decoded,
        }
    }

    /// `text` as a literal of `ty` holds it, when `ty` is built on a
    /// built-in type: a UUID, an instant or a date from its ISO text, text
    /// otherwise.
    pub fn text(&self, ty: &TypeRef, text: &str) -> Option<Constant> {
        let builtin = self.representation(ty)?;
        let decoded = constant::text(Some(builtin), text)?;
        Some(Self::kept_secret(self.chain(ty).as_ref(), decoded))
    }

    /// The fields whose parent is `parent`, in the order they are declared,
    /// copied fields where the line that copied them stands.
    fn fields(&self, parent: &Key, depth: usize) -> Vec<FieldPlan> {
        let Some(id) = self.graph.find(parent) else {
            return standard_variant_fields(parent);
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
                    wire_name,
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
                        wire: wire_name.clone(),
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
                    // The standard library's enums have no node; a sign-in
                    // provider's `SignInMethod` comes after the language's
                    // own (chunk A7b).
                    _ => jadpo_std::variant_ordinal(ty, variant).or_else(|| {
                        (ty == jadpo_std::SignInMethod::NAME)
                            .then(|| {
                                self.sign_in_providers
                                    .iter()
                                    .position(|name| name == variant)
                            })
                            .flatten()
                            .and_then(|at| {
                                u32::try_from(jadpo_std::SignInMethod::NAMES.len() + at).ok()
                            })
                    })?,
                };
                Some(ShapePlan {
                    ty: ty.to_string(),
                    open: self.is_providers(key),
                    variant: Some(VariantPlan {
                        name: variant.to_string(),
                        ordinal,
                    }),
                    fields: self.fields(key, depth),
                })
            }
            Kind::Type | Kind::Entity => Some(ShapePlan {
                ty: self.name(key).to_string(),
                open: self.is_providers(key),
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

/// The type the field `field` of the standard object type `host` holds
/// (area 19): what its part is built on, the standard type it is, the part
/// or `none`, or a list of it. A part with rules of its own (a whole
/// number's range, a line of text) is the field itself, whose chain
/// [`standard_part`] gives.
pub(crate) fn part_type(host: &str, field: &str, part: jadpo_std::Part) -> TypeRef {
    match part {
        jadpo_std::Part::Of(builtin) => TypeRef::Builtin(builtin),
        // A vector's item, a decimal to code (chunk 5.5s2).
        jadpo_std::Part::Float => TypeRef::Builtin(jadpo_std::Builtin::Decimal),
        jadpo_std::Part::Whole(..)
        | jadpo_std::Part::Bounded { .. }
        | jadpo_std::Part::Line
        | jadpo_std::Part::Matching(_)
        | jadpo_std::Part::Paragraph => {
            TypeRef::Field(Key::from_text(format!("field {host}.{field}")))
        }
        // A type built into the language held as a standard type (an
        // `Email`, a `Url`) is that built-in type, as a field of it is
        // (chunk 5.5w).
        jadpo_std::Part::Standard(name) => match jadpo_std::standard_type(name).map(|e| e.holds) {
            Some(jadpo_std::standard::Holds::Builtin(builtin)) => TypeRef::Builtin(builtin),
            _ => TypeRef::Std(name.to_string(), Vec::new()),
        },
        jadpo_std::Part::Optional(inner) => {
            TypeRef::Nullable(Box::new(part_type(host, field, *inner)))
        }
        jadpo_std::Part::List(inner) => TypeRef::List(Box::new(part_type(host, field, *inner))),
        jadpo_std::Part::Set(inner) => TypeRef::Set(Box::new(part_type(host, field, *inner))),
    }
}

/// The fields a variant of a standard choice carries (chunk 5.5i3), which
/// has no node: its entry's parts, each keyed `field <Choice>.<variant>.
/// <field>`, as an application's variant's fields are. None for any other
/// key.
fn standard_variant_fields(key: &Key) -> Vec<FieldPlan> {
    let fields = (key.kind() == Some(Kind::Variant))
        .then(|| key.name().split_once('.'))
        .flatten()
        .and_then(|(host, variant)| jadpo_std::standard::choice_variant(host, variant))
        .unwrap_or_default();
    fields
        .iter()
        .map(|(field, _)| FieldPlan {
            key: format!("field {}.{field}", key.name()),
            name: (*field).to_string(),
            validator: ValidatorId(u32::MAX),
            absent: None,
            omittable: false,
            none_left_out: false,
            section: None,
            wire: None,
        })
        .collect()
}

/// What the standard object type's field `key` names (`LocalTime.hour`),
/// or a standard choice's variant's (`Discount.amount_off.amount`, chunk
/// 5.5i3), is built on, and the rules its part keeps: a whole number's
/// range, a line's pattern, an amount's bounds; for a part that may be
/// `none` or is a list, its own part's.
fn standard_part(key: &Key) -> Option<(jadpo_std::Builtin, Rules)> {
    let (host, field) = key.name().split_once('.')?;
    let fields = match jadpo_std::standard_type(host)?.holds {
        jadpo_std::standard::Holds::Object(fields) => fields,
        jadpo_std::standard::Holds::Choice(_) => {
            let (variant, _) = field.split_once('.')?;
            jadpo_std::standard::choice_variant(host, variant)?
        }
        _ => return None,
    };
    let field = field.rsplit('.').next().unwrap_or(field);
    let mut part = fields.iter().find(|(name, _)| *name == field)?.1;
    while let jadpo_std::Part::Optional(inner)
    | jadpo_std::Part::List(inner)
    | jadpo_std::Part::Set(inner) = part
    {
        part = *inner;
    }
    // A point's coordinates are kept as the nearest doubles as the point
    // is built (area 20, question 2); the values are already in range.
    if let jadpo_std::Part::Standard(name) = part
        && jadpo_std::standard::geo::holds_double(name)
    {
        return Some((
            jadpo_std::Builtin::Decimal,
            Rules {
                double: true,
                ..Rules::default()
            },
        ));
    }
    Some((part.builtin()?, part.rules()))
}

/// A batch type's field's type (area 12, chunk 6.4).
pub(crate) fn batch_part(part: jadpo_std::batch::Part) -> TypeRef {
    use jadpo_std::batch::Part;
    match part {
        Part::Of(builtin) => TypeRef::Builtin(builtin),
        Part::Optional(builtin) => TypeRef::Nullable(Box::new(TypeRef::Builtin(builtin))),
        Part::Enum(name) => TypeRef::Std(name.to_string(), Vec::new()),
        Part::List(name) => TypeRef::List(Box::new(TypeRef::Std(name.to_string(), Vec::new()))),
        Part::Kept(name) => TypeRef::Std(name.to_string(), Vec::new()),
    }
}
