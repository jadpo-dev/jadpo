//! What each site of a body needs decoded before it runs: a literal's
//! constant, the number of the callable a call reaches, the shape and
//! slots a build fills, the failure a `reject` or an `attempt`'s arm
//! raises, and where a report hangs the site's value. Decoded once, for
//! both executors, so neither reads a type's chain or a field's key while a
//! body runs.

use jadpo_graph::body::{Body, Callee, Expr, Fallible, Node, Pattern, Site, Stmt};
use jadpo_graph::{Key, TypeRef};

use crate::constant::Constant;
use crate::types::{FailureId, ShapeId, Types, ValidatorId};

/// A decoded literal, by number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConstId(pub u32);

/// One site, decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SitePlan {
    /// Where a report hangs this site's value (area 13, "What a failed
    /// test reports"): an operator's first byte for arithmetic,
    /// comparisons and `and`/`or`, a field's name for a field read,
    /// otherwise the site's start.
    pub anchor: u32,
    pub decoded: Decoded,
}

/// What a site needs that isn't control flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decoded {
    /// Nothing beyond the typed body.
    Nothing,
    /// A literal; `None` when its type can't hold it, which the checker
    /// rules out.
    Constant(Option<ConstId>),
    /// A call to a function, action, fixture or library function, by the
    /// number the plan gives it; `None` when it has no body.
    Callable(Option<u32>),
    /// `Type(value)`: the type's rules, and whether a broken rule is a
    /// failure its `attempt` maps rather than something the checker proved
    /// can't happen. `set` for a named set type, which a list becomes by
    /// dropping its repeats, in the canonical order (area 2, "Named
    /// collection construction").
    Construct {
        validator: ValidatorId,
        fallible: bool,
        set: bool,
    },
    /// `Type { ... }` or `Enum.variant { ... }`.
    Build {
        shape: Option<ShapeId>,
        /// Each field given, in the order written.
        fields: Vec<Given>,
        fallible: bool,
    },
    /// A list written where a list or a set goes.
    List { set: bool, validator: ValidatorId },
    /// `value.field`: the field's own name, as a value holds it.
    Field { name: String },
    /// `reject Failure { ... }`.
    Reject {
        failure: Option<FailureId>,
        /// Each field given, in the order written: its place in the
        /// failure's fields.
        slots: Vec<Option<u32>>,
    },
    /// An `attempt`'s arms, in the order written: the failure each
    /// raises, and where each of its given fields goes.
    Attempt { arms: Vec<Option<Raise>> },
    /// A value `match`'s patterns, arm by arm.
    Match { arms: Vec<ArmPattern> },
    /// `assert … fails Failure`: the failure's name now.
    AssertFails { expected: String },
    /// A call to a query, by the number of its plan; `None` when it has
    /// none, which the checker rules out.
    Query(Option<u32>),
    /// `Entity.get(id)`, by the number of the entity's plan.
    Get(Option<u32>),
    /// `create`, `update` or `delete`: the number of the write's plan, and
    /// the failure each `conflict:` arm raises, with where each of its
    /// given fields goes.
    Write {
        plan: Option<u32>,
        arms: Vec<Option<Raise>>,
    },
    /// `config.<name>`: the number of the setting in the plan's settings;
    /// `None` when it has none, which the checker rules out.
    Config(Option<u32>),
    /// Something a later area builds, as written up to its arguments or
    /// block: `clock.now`, `query open_todos`, `Todo.update`.
    Written(String),
}

/// One field given to a build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Given {
    /// Its place in the shape; `None` for a field the shape hasn't, which
    /// the checker rules out.
    pub slot: Option<u32>,
    /// Its own name, for the rule a construction reports as broken.
    pub name: String,
    pub validator: ValidatorId,
}

/// The failure an `attempt`'s arm raises.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Raise {
    pub failure: FailureId,
    pub slots: Vec<Option<u32>>,
}

/// What a value `match` arm's pattern compares with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArmPattern {
    /// A pattern that needs nothing decoded.
    Other,
    /// `Enum.variant`, by the names a value holds.
    Variant { ty: String, name: String },
    /// A literal, decoded as the subject's type holds it.
    Literal(Option<ConstId>),
}

/// Decodes a body's sites, adding constants and type plans as it goes.
pub(crate) struct Decoder<'a> {
    pub types: &'a mut Types,
    pub constants: &'a mut Vec<Constant>,
    /// The number of the callable with each key.
    pub callable: &'a dyn Fn(&Key) -> Option<u32>,
    /// The number of each query's plan, by its key.
    pub query: &'a dyn Fn(&Key) -> Option<u32>,
    /// The number of each entity's plan, by its key.
    pub entity: &'a dyn Fn(&Key) -> Option<u32>,
    /// The number of the plan of the write at each site of this body.
    pub write: &'a dyn Fn(u32) -> Option<u32>,
    /// The number of each setting, by its key (area 10).
    pub setting: &'a dyn Fn(&Key) -> Option<u32>,
}

impl Decoder<'_> {
    fn constant(&mut self, constant: Option<Constant>) -> Option<ConstId> {
        let constant = constant?;
        let id = ConstId(self.constants.len() as u32);
        self.constants.push(constant);
        Some(id)
    }

    fn slots(
        &mut self,
        failure: Option<FailureId>,
        fields: &[(Key, jadpo_graph::body::SiteId)],
    ) -> Vec<Option<u32>> {
        let plan = failure.and_then(|id| self.types.failure(id));
        fields
            .iter()
            .map(|(key, _)| plan.and_then(|plan| plan.slot(key.as_str())))
            .collect()
    }

    /// Every site of `body` (in the file whose text is `text`), decoded.
    pub fn body(&mut self, body: &Body, text: &str) -> Vec<SitePlan> {
        body.sites
            .iter()
            .enumerate()
            .map(|(index, site)| SitePlan {
                anchor: anchor(self.types, body, site, text),
                decoded: self.site(body, index as u32, site, text),
            })
            .collect()
    }

    fn site(&mut self, body: &Body, index: u32, site: &Site, text: &str) -> Decoded {
        match &site.node {
            Node::Expr(Expr::Literal(literal)) => {
                let constant = self.types.literal(literal, &site.ty);
                Decoded::Constant(self.constant(constant))
            }
            Node::Expr(Expr::Call {
                callee, fallible, ..
            }) => match callee {
                Callee::Function(key)
                | Callee::Action(key)
                | Callee::Fixture(key)
                | Callee::LibraryFunction(key) => Decoded::Callable((self.callable)(key)),
                Callee::Construct(ty) => Decoded::Construct {
                    validator: self.types.validator_id(ty),
                    fallible: matches!(fallible, Fallible::Construction { .. }),
                    set: self.types.is_set(ty),
                },
                Callee::Query(key) => Decoded::Query((self.query)(key)),
                Callee::Entity { entity, operation } if operation == "get" => {
                    Decoded::Get((self.entity)(entity))
                }
                Callee::Entity { entity, operation } => {
                    Decoded::Written(format!("{}.{operation}", self.types.name(entity)))
                }
                Callee::Std(_)
                | Callee::Object(_)
                | Callee::Variant(_)
                | Callee::Caller(_)
                | Callee::Session(_)
                | Callee::AdvanceClock
                | Callee::Unknown => Decoded::Nothing,
            },
            Node::Expr(Expr::Build {
                callee: Callee::Object(key) | Callee::Variant(key),
                fields,
                fallible,
            }) => {
                let shape = self.types.shape_id(key);
                let given = fields
                    .iter()
                    .map(|(field, _)| Given {
                        slot: shape
                            .and_then(|id| self.types.shape(id))
                            .and_then(|plan| plan.slot(field.as_str())),
                        name: self.types.field_name(field).to_string(),
                        validator: self.types.validator_id(&TypeRef::Field(field.clone())),
                    })
                    .collect();
                Decoded::Build {
                    shape,
                    fields: given,
                    fallible: matches!(fallible, Fallible::Construction { .. }),
                }
            }
            Node::Expr(Expr::Write { conflicts, .. }) => Decoded::Write {
                plan: (self.write)(index),
                arms: conflicts
                    .iter()
                    .map(|arm| {
                        let failure = self.types.failure_id(&arm.failure)?;
                        Some(Raise {
                            failure,
                            slots: self.slots(Some(failure), &arm.fields),
                        })
                    })
                    .collect(),
            },
            Node::Expr(Expr::List { .. }) => Decoded::List {
                set: self.types.is_set(&site.ty),
                validator: self.types.validator_id(&site.ty),
            },
            Node::Expr(Expr::Field { field, .. }) => Decoded::Field {
                name: self.types.field_name(field).to_string(),
            },
            Node::Expr(Expr::Reject {
                failure: Some(key),
                fields,
            }) => {
                let failure = self.types.failure_id(key);
                Decoded::Reject {
                    failure,
                    slots: self.slots(failure, fields),
                }
            }
            Node::Expr(Expr::Attempt { arms, .. }) => Decoded::Attempt {
                arms: arms
                    .iter()
                    .map(|arm| {
                        let failure = self.types.failure_id(&arm.failure)?;
                        Some(Raise {
                            failure,
                            slots: self.slots(Some(failure), &arm.fields),
                        })
                    })
                    .collect(),
            },
            Node::Expr(Expr::Match {
                subject,
                outcome: false,
                arms,
            }) => {
                let ty = subject
                    .and_then(|subject| body.sites.get(subject.0 as usize))
                    .map_or(TypeRef::None, |subject| subject.ty.clone());
                Decoded::Match {
                    arms: arms
                        .iter()
                        .map(|arm| match &arm.pattern {
                            Pattern::Variant(key) => match self.types.shape_plan(key) {
                                Some(shape) => ArmPattern::Variant {
                                    ty: shape.ty,
                                    name: shape.variant.map(|v| v.name).unwrap_or_default(),
                                },
                                None => ArmPattern::Other,
                            },
                            Pattern::Literal(literal) => {
                                let constant = self.types.literal(literal, &ty);
                                ArmPattern::Literal(self.constant(constant))
                            }
                            _ => ArmPattern::Other,
                        })
                        .collect(),
                }
            }
            Node::Stmt(Stmt::Assert {
                fails: Some(TypeRef::Node(expected)),
                ..
            }) => Decoded::AssertFails {
                expected: self.types.name(expected).to_string(),
            },
            Node::Expr(Expr::Config { field }) => Decoded::Config((self.setting)(field)),
            Node::Expr(Expr::Unchecked) => Decoded::Written(written(site, text)),
            _ => Decoded::Nothing,
        }
    }
}

/// What a site is written as, up to its arguments or its block: what a
/// fault for something not built yet names (`clock.now`, `Todo.update`).
fn written(site: &Site, text: &str) -> String {
    let source = text
        .get(site.start as usize..site.end as usize)
        .unwrap_or_default();
    let line = source.lines().next().unwrap_or_default();
    let end = line.find(['(', '{']).unwrap_or(line.len());
    line[..end].trim().to_string()
}

/// Where a site's value points in a report: an operator, a field's name,
/// or the site's start.
fn anchor(types: &Types, body: &Body, site: &Site, text: &str) -> u32 {
    match &site.node {
        Node::Expr(Expr::Binary { lhs, rhs, .. }) => {
            let (Some(lhs), Some(rhs)) = (
                body.sites.get(lhs.0 as usize),
                body.sites.get(rhs.0 as usize),
            ) else {
                return site.start;
            };
            text.get(lhs.end as usize..rhs.start as usize)
                .and_then(|between| between.find(|c: char| !c.is_whitespace()))
                .map_or(site.start, |offset| lhs.end + offset as u32)
        }
        Node::Expr(Expr::Field { field, .. }) => {
            let name = types.field_name(field);
            match text.get(site.start as usize..site.end as usize) {
                Some(written) if written.ends_with(name) => site.end - name.len() as u32,
                _ => site.start,
            }
        }
        _ => site.start,
    }
}
