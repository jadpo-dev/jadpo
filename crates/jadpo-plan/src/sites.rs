//! What each site of a body needs decoded before it runs: a literal's
//! constant, the number of the callable a call reaches, the shape and
//! slots a build fills, the failure a `reject` or an `attempt`'s arm
//! raises, and where a report hangs the site's value. Decoded once, for
//! both executors, so neither reads a type's chain or a field's key while a
//! body runs.

use jadpo_graph::body::{
    BinaryOp, Body, Callee, Expr, Fallible, LocalId, Node, Pattern, Site, SiteId, Stmt,
};
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
        /// An object type's validator, when it has rules across its fields
        /// (area 2, "Rules across fields"), and whether one of them can
        /// break here, so the value is checked once it is built; one the
        /// checker proved is checked only under proof checks.
        across: Option<(ValidatorId, bool)>,
    },
    /// A list written where a list or a set goes, or `[]` where a map
    /// goes, which is an empty map (area 19, question 8).
    List {
        set: bool,
        map: bool,
        validator: ValidatorId,
    },
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
    /// `*` or `/` of an amount of money by a number: how the result is
    /// rounded to a whole minor unit, the amount's type's `rounding:`
    /// (area 19).
    Rounding(jadpo_std::Rounding),
    /// `json.read` or `json.write` (area 19, "Data formats"): the number
    /// of the type it reads or writes in the plan's JSON table
    /// ([`crate::Plan::json`]), whose codec is the wire's for that type.
    Json(u32),
    /// `prompts.render` (area 21): the prompt's place in the plan's
    /// prompts, and for each field its text writes, in the entry's order,
    /// the number of its type in the JSON table, whose codec writes a
    /// value's text.
    Prompt { entry: u32, fields: Vec<u32> },
    /// `batches.each` (area 12, chunk 6.4): its place in the plan's
    /// batches ([`crate::Plan::batches`]), and the sites the action works
    /// out as it starts the run, in the order the run saves them: each of
    /// `from:`'s query's arguments, then each value `each:` and
    /// `finished:` capture, then `key:`.
    Batch { number: u32, sites: Vec<u32> },
    /// Something a later area builds, as written up to its arguments or
    /// block: `query open_todos`, `Todo.update`.
    Written(String),
}

/// A `batches.each` call as its site decodes it, before the plan numbers
/// the read and the store its run goes through (area 12, chunk 6.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BatchSite {
    /// `action catalogue.recount#1`.
    pub name: String,
    pub shape: String,
    /// The query `from:` calls, by its plan's number.
    pub query: Option<u32>,
    /// The actions `each:` and `finished:` call, by key.
    pub each: Option<(Key, jadpo_std::batch::BatchCall)>,
    pub finished: Option<(Key, jadpo_std::batch::BatchCall)>,
    pub captured: Vec<u32>,
    /// The type of each value in `captured`, then `key:`'s, which decide
    /// the run's tenant (chunk 6.4c).
    pub saved_types: Vec<TypeRef>,
    pub key_type: Option<TypeRef>,
    pub from_args: usize,
    pub key: Option<u32>,
    /// For a file helper's read (chunk 6.4b): its format and the JSON
    /// table's number of the row type, each row read as one.
    pub file: Option<(jadpo_std::batch::RowFormat, u32, TypeRef)>,
    /// For `json.read`'s file form, the members `items_at:` names; and for
    /// any read, `max_unpacked_size:`'s bytes (chunk 6.4b3).
    pub items_at: Vec<String>,
    pub max_unpacked: Option<u64>,
    /// For a file helper's write (chunk 6.4b2): what it writes, and where.
    pub write: Option<WriteSite>,
    /// The action or fixture starting it, by name.
    pub starter: String,
}

/// A file helper's write as its site decodes it (chunk 6.4b2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WriteSite {
    pub format: jadpo_std::batch::RowFormat,
    /// The `files` connection it writes to.
    pub store: String,
    /// The JSON table's number of the row type, and the type, whose
    /// fields' columns the plan reads.
    pub row: u32,
    pub row_ty: TypeRef,
    /// The places in `captured` of `name:` and `format:`.
    pub name: u32,
    pub csv_format: Option<u32>,
    pub each: jadpo_std::batch::BatchRow,
}

/// The path from the inline parameter `item` to the value at `site`: the
/// item itself, or a field inside it, `row.sku` being `["sku"]`; `None`
/// for anything else.
fn item_path(types: &Types, body: &Body, site: SiteId, item: LocalId) -> Option<Vec<String>> {
    match &body.site(site).node {
        Node::Expr(Expr::Local(local)) if *local == item => Some(Vec::new()),
        Node::Expr(Expr::Field { receiver, field }) => {
            let mut path = item_path(types, body, *receiver, item)?;
            path.push(types.field_name(field).to_string());
            Some(path)
        }
        _ => None,
    }
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
    /// The place among the failure's fields of its `problem:
    /// DecodeProblem`, which receives `json.read`'s first problem when the
    /// arm maps `mismatch` (area 19, "Data formats").
    pub problem: Option<u32>,
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
/// A prompt's place in the plan's prompts, and the type of each field its
/// text writes, in its entry's order (area 21).
pub(crate) type PromptFields = (u32, Vec<TypeRef>);

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
    /// The types `json.read` reads and `json.write` writes, numbered as
    /// they are met (area 19, "Data formats").
    pub json: &'a mut Vec<TypeRef>,
    /// Each prompt's place in the plan's prompts, by its name, and the type
    /// of each field its text writes (area 21).
    pub prompt: &'a dyn Fn(&str) -> Option<PromptFields>,
    /// The callable whose body this is, and the program's `batches.each`
    /// calls so far, which its own are added to (area 12).
    pub starter: &'a Key,
    pub batches: &'a mut Vec<BatchSite>,
}

impl Decoder<'_> {
    fn constant(&mut self, constant: Option<Constant>) -> Option<ConstId> {
        let constant = constant?;
        let id = ConstId(self.constants.len() as u32);
        self.constants.push(constant);
        Some(id)
    }

    /// The number of `ty` in the JSON table, numbering it when it is new.
    fn json_type(&mut self, ty: TypeRef) -> Decoded {
        let at = match self.json.iter().position(|known| *known == ty) {
            Some(at) => at,
            None => {
                self.json.push(ty);
                self.json.len() - 1
            }
        };
        Decoded::Json(at as u32)
    }

    /// `batches.each(from:, each:, finished:, key:)`, or a file helper's
    /// read (`csv.read(file, as:, each:, finished:, key:)`, chunk 6.4b) or
    /// write (`csv.write(store, name:, format:, from:, each:, finished:,
    /// key:)`, chunk 6.4b2): what its run saves and calls, added to the
    /// program's batches.
    fn batch(
        &mut self,
        body: &Body,
        args: &[Option<SiteId>],
        op: jadpo_std::batch::BatchOp,
    ) -> Decoded {
        use jadpo_std::batch::{BatchArg, BatchCall, BatchOp};
        let read = match op {
            BatchOp::Read(format) => Some(format),
            _ => None,
        };
        let write = match op {
            BatchOp::Write(format) => Some(format),
            _ => None,
        };
        let (from_at, each_at, finished_at) = jadpo_check::types::run_places(op);
        let node = |site: Option<SiteId>| site.map(|site| (site, body.site(site)));
        let mut sites: Vec<u32> = Vec::new();
        let mut captured: Vec<u32> = Vec::new();
        let mut saved_types: Vec<TypeRef> = Vec::new();
        let mut shape = String::new();
        let mut query = None;
        let mut query_key: Option<Key> = None;
        if let Some((_, from)) = node(from_at.and_then(|at| args.get(at).copied().flatten()))
            && let Node::Expr(Expr::Call {
                callee: Callee::Query(key),
                args: given,
                ..
            }) = &from.node
        {
            query = (self.query)(key);
            query_key = Some(key.clone());
            shape.push_str(&format!("from {}(", key.as_str()));
            for arg in given.iter().flatten() {
                let ty = body.site(*arg).ty.clone();
                shape.push_str(&format!("{},", ty.canonical()));
                if let Decoded::Json(number) = self.json_type(ty.clone()) {
                    captured.push(number);
                    saved_types.push(ty);
                    sites.push(arg.0);
                }
            }
            shape.push(')');
        }
        // A file helper's file, saved with the run first.
        if let Some(format) = read
            && let Some((at, file)) = node(args.first().copied().flatten())
        {
            let ty = file.ty.clone();
            shape.push_str(&format!("{}.read {},", format.namespace(), ty.canonical()));
            if let Decoded::Json(number) = self.json_type(ty.clone()) {
                captured.push(number);
                saved_types.push(ty);
                sites.push(at.0);
            }
        }
        let from_args = captured.len();
        // A writer's store, and its `name:` and `format:`, saved next.
        let mut written_to: Option<(String, u32, Option<u32>)> = None;
        if let Some(format) = write {
            let store = match node(args.first().copied().flatten()) {
                Some((_, site)) => match &site.node {
                    Node::Expr(Expr::Literal(jadpo_std::Literal::Text(name))) => name.clone(),
                    _ => String::new(),
                },
                None => String::new(),
            };
            shape.push_str(&format!("{}.write {store}", format.namespace()));
            let mut places: Vec<Option<u32>> = Vec::new();
            for at in [1, 2] {
                let Some((site, held)) = node(args.get(at).copied().flatten()) else {
                    places.push(None);
                    continue;
                };
                shape.push_str(&format!(" {}", held.ty.canonical()));
                match self.json_type(held.ty.clone()) {
                    Decoded::Json(number) => {
                        places.push(Some(captured.len() as u32));
                        captured.push(number);
                        saved_types.push(held.ty.clone());
                        sites.push(site.0);
                    }
                    _ => places.push(None),
                }
            }
            if let Some(Some(name)) = places.first().copied() {
                written_to = Some((store, name, places.get(1).copied().flatten()));
            }
        }
        // Where `each:` and the rest sit: after a file helper's file and
        // `as:`, or a writer's store, `name:` and `format:`.
        let calls = each_at;
        let called = |at: Option<SiteId>,
                      word: &str,
                      this: &mut Decoder<'_>,
                      sites: &mut Vec<u32>,
                      captured: &mut Vec<u32>,
                      saved_types: &mut Vec<TypeRef>,
                      shape: &mut String|
         -> Option<(Key, BatchCall)> {
            let (_, inline) = node(at)?;
            let Node::Expr(Expr::Inline {
                params,
                body: inner,
            }) = &inline.node
            else {
                return None;
            };
            let item = *params.first()?;
            let (_, call) = node(*inner)?;
            let Node::Expr(Expr::Call {
                callee: Callee::Action(key),
                args: given,
                ..
            }) = &call.node
            else {
                return None;
            };
            let callable = (this.callable)(key)?;
            shape.push_str(&format!(" {word} {}(", key.as_str()));
            let mut out = Vec::new();
            for arg in given.iter().flatten() {
                if let Some(path) = item_path(this.types, body, *arg, item) {
                    let fields: String = path.iter().map(|field| format!(".{field}")).collect();
                    shape.push_str(&format!("item{fields},"));
                    out.push(BatchArg::Item(path));
                    continue;
                }
                let ty = body.site(*arg).ty.clone();
                shape.push_str(&format!("{},", ty.canonical()));
                let Decoded::Json(number) = this.json_type(ty.clone()) else {
                    return None;
                };
                out.push(BatchArg::Captured(captured.len() as u32));
                captured.push(number);
                saved_types.push(ty);
                sites.push(arg.0);
            }
            shape.push(')');
            Some((
                key.clone(),
                BatchCall {
                    callable,
                    args: out,
                },
            ))
        };
        // `json.read`'s `items_at:` and any read's `max_unpacked_size:`,
        // both written out, the plan's rather than saved (chunk 6.4b3).
        // Where the list sits is part of the run's shape: a position in
        // one list means nothing in another.
        let written = |at: usize| match node(args.get(at).copied().flatten()) {
            Some((
                _,
                Site {
                    node: Node::Expr(Expr::Literal(jadpo_std::Literal::Text(text))),
                    ..
                },
            )) => Some(text.clone()),
            _ => None,
        };
        let (items_at, max_unpacked) = match read {
            Some(_) => (
                written(jadpo_std::batch::ITEMS_AT)
                    .map(|path| path.split('.').map(str::to_string).collect::<Vec<_>>())
                    .unwrap_or_default(),
                written(jadpo_std::batch::MAX_UNPACKED_SIZE)
                    .and_then(|size| jadpo_std::files::size(&size)),
            ),
            None => (Vec::new(), None),
        };
        if !items_at.is_empty() {
            shape.push_str(&format!(" items_at {}", items_at.join(".")));
        }
        // A file helper's row type: what `each:`'s item is.
        let row_read = read.and_then(|format| {
            let (_, inline) = node(args.get(calls).copied().flatten())?;
            let Node::Expr(Expr::Inline { params, .. }) = &inline.node else {
                return None;
            };
            let local = body.locals.get(params.first()?.0 as usize)?;
            let ty = local.ty.clone();
            shape.push_str(&format!(" as {}", ty.canonical()));
            match self.json_type(ty.clone()) {
                Decoded::Json(number) => Some((format, number, ty)),
                _ => None,
            }
        });
        // A writer's row: the inline function's body in this callable's
        // body, each local of it the body reads saved with the run.
        let row = match (write, node(args.get(each_at).copied().flatten())) {
            (
                Some(format),
                Some((
                    _,
                    Site {
                        node:
                            Node::Expr(Expr::Inline {
                                params,
                                body: Some(inner),
                            }),
                        ..
                    },
                )),
            ) => {
                let row_ty = body.site(*inner).ty.clone();
                shape.push_str(&format!(" each {}(", row_ty.canonical()));
                let mut locals = Vec::new();
                let mut whole = true;
                for (local, at) in
                    body.outer_reads(args.get(each_at).copied().flatten().unwrap_or(*inner))
                {
                    let ty = body.site(at).ty.clone();
                    shape.push_str(&format!("{},", ty.canonical()));
                    match self.json_type(ty.clone()) {
                        Decoded::Json(number) => {
                            locals.push((local.0, captured.len() as u32));
                            captured.push(number);
                            saved_types.push(ty);
                            sites.push(at.0);
                        }
                        _ => whole = false,
                    }
                }
                shape.push(')');
                let number = match self.json_type(row_ty.clone()) {
                    Decoded::Json(number) => Some(number),
                    _ => None,
                };
                match (number, params.first(), (self.callable)(self.starter), whole) {
                    (Some(number), Some(item), Some(callable), true) => Some((
                        format,
                        number,
                        row_ty,
                        jadpo_std::batch::BatchRow {
                            callable,
                            body: inner.0,
                            item: item.0,
                            locals,
                        },
                    )),
                    _ => None,
                }
            }
            _ => None,
        };
        let each = if write.is_some() {
            None
        } else {
            called(
                args.get(calls).copied().flatten(),
                "each",
                self,
                &mut sites,
                &mut captured,
                &mut saved_types,
                &mut shape,
            )
        };
        let finished = called(
            args.get(finished_at).copied().flatten(),
            "finished",
            self,
            &mut sites,
            &mut captured,
            &mut saved_types,
            &mut shape,
        );
        let key_type =
            node(args.get(finished_at + 1).copied().flatten()).map(|(_, site)| site.ty.clone());
        let key = match node(args.get(finished_at + 1).copied().flatten()) {
            Some((at, site)) => {
                shape.push_str(&format!(" key {}", site.ty.canonical()));
                sites.push(at.0);
                match self.json_type(site.ty.clone()) {
                    Decoded::Json(number) => Some(number),
                    _ => None,
                }
            }
            None => None,
        };
        // Named by what the call is, in identity keys that survive a
        // rename (chunk 6.8): the action starting it, the query `from:`
        // walks and the action `each:` runs, so adding or moving another
        // call leaves its name as it was. Only a second call of the same
        // three in one action is told apart by its place among them.
        let mut name = self.starter.as_str().to_string();
        if let Some(query) = query_key {
            name.push_str(&format!(" from {}", query.as_str()));
        }
        if let Some((format, _, row)) = &row_read {
            name.push_str(&format!(
                " reads {} {}",
                format.namespace(),
                row.canonical()
            ));
        }
        if let Some((action, _)) = &each {
            name.push_str(&format!(" each {}", action.as_str()));
        }
        if let Some((format, _, row_ty, _)) = &row {
            name.push_str(&format!(
                " writes {} {}",
                format.namespace(),
                row_ty.canonical()
            ));
        }
        let write = match (row, written_to) {
            (Some((format, row, row_ty, each)), Some((store, name, csv_format))) => {
                Some(WriteSite {
                    format,
                    store,
                    row,
                    row_ty,
                    name,
                    csv_format,
                    each,
                })
            }
            _ => None,
        };
        let alike = self
            .batches
            .iter()
            .filter(|batch| batch.name == name || batch.name.starts_with(&format!("{name} #")))
            .count();
        if alike > 0 {
            name.push_str(&format!(" #{}", alike + 1));
        }
        let number = self.batches.len() as u32;
        self.batches.push(BatchSite {
            name,
            shape,
            query,
            each,
            finished,
            captured,
            saved_types,
            key_type,
            from_args,
            key,
            file: row_read,
            items_at,
            max_unpacked,
            write,
            starter: self.starter.name().to_string(),
        });
        Decoded::Batch { number, sites }
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
                callee,
                fallible,
                args,
            }) => match callee {
                Callee::Function(key)
                | Callee::Action(key)
                | Callee::Fixture(key)
                | Callee::LibraryFunction(key) => Decoded::Callable((self.callable)(key)),
                // A test's run of a schedule calls the action it runs (area
                // 13; chunk 6.2c).
                Callee::Schedule { action, .. } => Decoded::Callable((self.callable)(action)),
                Callee::Construct(ty) => Decoded::Construct {
                    validator: self.types.validator_id(ty),
                    fallible: matches!(fallible, Fallible::Construction { .. }),
                    set: self.types.is_set(ty),
                },
                // `text.parse` gives the type `as:` names, whose rules are
                // checked once the text is read (area 19).
                Callee::Std(jadpo_std::Op::TextParse) => Decoded::Construct {
                    validator: self.types.validator_id(&site.ty),
                    fallible: matches!(fallible, Fallible::Construction { .. }),
                    set: false,
                },
                // JSON in memory (area 19): the type read, which is the
                // call's, or the type of the value written.
                Callee::Std(jadpo_std::Op::JsonRead) => self.json_type(site.ty.clone()),
                // A token's own claims, read as the type `as:` names, the
                // call's (chunk 5.5r4).
                Callee::Std(jadpo_std::Op::CryptoVerifyJwt)
                    if args.get(4).copied().flatten().is_some() =>
                {
                    self.json_type(site.ty.clone())
                }
                Callee::Std(jadpo_std::Op::JsonWrite) => {
                    let written = args
                        .first()
                        .copied()
                        .flatten()
                        .and_then(|arg| body.sites.get(arg.0 as usize))
                        .map_or(TypeRef::None, |arg| arg.ty.clone());
                    self.json_type(written)
                }
                Callee::Prompt(key) => match (self.prompt)(key) {
                    Some((entry, fields)) => Decoded::Prompt {
                        entry,
                        fields: fields
                            .into_iter()
                            .map(|ty| match self.json_type(ty) {
                                Decoded::Json(number) => number,
                                _ => 0,
                            })
                            .collect(),
                    },
                    None => Decoded::Nothing,
                },
                Callee::Query(key) => Decoded::Query((self.query)(key)),
                Callee::Entity { entity, operation } if operation == "get" => {
                    Decoded::Get((self.entity)(entity))
                }
                Callee::Entity { entity, operation } => {
                    Decoded::Written(format!("{}.{operation}", self.types.name(entity)))
                }
                Callee::Batch(op) if op.starts() => self.batch(body, args, *op),
                Callee::Std(_)
                | Callee::Object(_)
                | Callee::Variant(_)
                | Callee::Caller(_)
                | Callee::Session(_)
                | Callee::Batch(_)
                | Callee::Files { .. }
                | Callee::AdvanceClock
                | Callee::Translation(_)
                | Callee::Unknown => Decoded::Nothing,
            },
            Node::Expr(Expr::Build {
                callee: Callee::Object(key) | Callee::Variant(key),
                fields,
                fallible,
            }) => {
                // A standard object type (area 19) has no declaration: its
                // shape is its data entry's fields, found by name.
                let standard = self.types.standard_object_shape(key);
                let shape = standard.or_else(|| self.types.shape_id(key));
                let given = fields
                    .iter()
                    .map(|(field, _)| Given {
                        slot: shape.and_then(|id| self.types.shape(id)).and_then(|plan| {
                            if standard.is_some() {
                                plan.slot_named(self.types.field_name(field))
                            } else {
                                plan.slot(field.as_str())
                            }
                        }),
                        name: self.types.field_name(field).to_string(),
                        validator: self.types.validator_id(&TypeRef::Field(field.clone())),
                    })
                    .collect();
                // A standard object's rules across its parts are its data
                // entry's (area 19, chunk 5.5t2).
                let object = if standard.is_some() {
                    TypeRef::Std(key.name().to_string(), Vec::new())
                } else {
                    TypeRef::Node(key.clone())
                };
                let across = (key.kind() == Some(jadpo_graph::Kind::Type)
                    && !self.types.across(&object).is_empty())
                .then(|| {
                    let breaks = matches!(fallible, Fallible::Construction { named, .. } if !named.is_empty());
                    (self.types.validator_id(&object), breaks)
                });
                Decoded::Build {
                    shape,
                    fields: given,
                    fallible: matches!(fallible, Fallible::Construction { rules, .. } if !rules.is_empty()),
                    across,
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
                            problem: None,
                        })
                    })
                    .collect(),
            },
            Node::Expr(Expr::List { .. }) => Decoded::List {
                set: self.types.is_set(&site.ty),
                map: self.types.is_map(&site.ty),
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
                        let problem = arm.problem.as_ref().and_then(|field| {
                            self.types
                                .failure(failure)
                                .and_then(|plan| plan.slot(field.as_str()))
                        });
                        Some(Raise {
                            failure,
                            slots: self.slots(Some(failure), &arm.fields),
                            problem,
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
            // An amount scaled by a number rounds by its type's `rounding:`,
            // `half_even` when it declares none (area 19).
            Node::Expr(Expr::Binary {
                op: BinaryOp::Multiply | BinaryOp::Divide,
                lhs,
                ..
            }) => {
                let rounding = body
                    .sites
                    .get(lhs.0 as usize)
                    .and_then(|lhs| self.types.chain(&lhs.ty))
                    .filter(|(builtin, _)| *builtin == jadpo_std::Builtin::Money)
                    .and_then(|(_, rules)| rules.iter().find_map(|rules| rules.rounding));
                match rounding {
                    Some(rounding) => Decoded::Rounding(rounding),
                    None => Decoded::Nothing,
                }
            }
            // A constant runs its own body, which gives its value.
            Node::Expr(Expr::Constant { constant }) => Decoded::Callable((self.callable)(constant)),
            Node::Expr(Expr::Unchecked) => Decoded::Written(written(site, text)),
            _ => Decoded::Nothing,
        }
    }
}

/// What a site is written as, up to its arguments or its block: what a
/// fault for something not built yet names (`Todo.update`).
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
