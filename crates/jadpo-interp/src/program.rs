//! A checked project ready to run: every callable's typed body, the places
//! its frames name, and what its types are made of, copied out of the
//! checker's queries once so running needs no database.

use std::collections::HashMap;
use std::fmt;

use jadpo_check::types::{callables_of, typed_body};
use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Severity};
use jadpo_graph::body::Body;
use jadpo_graph::{Key, Kind};
use jadpo_runtime::{CallablePlace, Places};
use jadpo_value::Value;

use crate::Outcome;
use crate::eval::Interp;
use crate::report::Report;
use crate::types::Types;

/// One callable with a body.
#[derive(Debug)]
pub(crate) struct Callable {
    pub key: Key,
    pub kind: Kind,
    pub file: String,
    pub body: std::sync::Arc<Body>,
}

/// A project that checked without errors, ready to run.
#[derive(Debug)]
pub struct Program {
    pub(crate) callables: Vec<Callable>,
    by_key: HashMap<Key, u32>,
    pub(crate) places: Places,
    pub(crate) types: Types,
    /// Each file's text, by project-relative path, for reports.
    pub(crate) texts: HashMap<String, String>,
    /// Whether constructions the checker proved are checked again as they
    /// run ([`Program::with_proof_checks`]).
    pub(crate) proof_checks: bool,
}

/// Where a frame stands: its callable's file, the bytes of its site and
/// the line the site starts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SiteSpan<'a> {
    pub file: &'a str,
    pub start: u32,
    pub end: u32,
    pub line: u32,
}

/// Why a call couldn't start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallError {
    /// No callable with a body has this key or name.
    Unknown(String),
    /// It takes a different number of values.
    Arity {
        callable: String,
        expected: usize,
        given: usize,
    },
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallError::Unknown(name) => {
                write!(f, "Nothing called `{name}` has a body that can run")
            }
            CallError::Arity {
                callable,
                expected,
                given,
            } => write!(f, "`{callable}` takes {expected} values, not {given}"),
        }
    }
}

impl Program {
    /// Reads every callable of `project` out of its checked queries. A
    /// project with errors doesn't run: its errors come back instead.
    pub fn load(db: &dyn Db, project: Project) -> Result<Program, Vec<Diagnostic>> {
        let errors: Vec<Diagnostic> = jadpo_check::check(db, project)
            .into_iter()
            .filter(|d| d.severity() == Severity::Error)
            .collect();
        if !errors.is_empty() {
            return Err(errors);
        }
        let graph = jadpo_check::graph::graph(db, project).clone();
        let mut found: Vec<Callable> = Vec::new();
        let mut texts = HashMap::new();
        for &file in project.files(db) {
            let path = file.path(db).clone();
            texts.insert(path.clone(), file.text(db).clone());
            for callable in callables_of(db, file) {
                let checked = typed_body(db, project, callable);
                let Some(key) = checked.key.clone() else {
                    continue;
                };
                let Some(kind) = key.kind() else { continue };
                found.push(Callable {
                    key,
                    kind,
                    file: path.clone(),
                    body: checked.body.clone(),
                });
            }
        }
        found.sort_by(|a, b| a.key.cmp(&b.key));
        found.dedup_by(|a, b| a.key == b.key);
        let mut places = Places::new();
        let mut by_key = HashMap::new();
        for callable in &found {
            let node = graph.get(&callable.key);
            let name = node.map_or(callable.key.name(), |node| node.name.as_str());
            let label = match callable.kind {
                Kind::Test => format!("test \"{name}\""),
                kind => format!("{} {name}", kind.word()),
            };
            let line = node
                .and_then(|node| node.place.as_ref())
                .map_or(1, |place| place.lines.0);
            let id = places.push(CallablePlace {
                label,
                file: callable.file.clone(),
                line,
                sites: callable.body.sites.iter().map(|site| site.line).collect(),
            });
            by_key.insert(callable.key.clone(), id);
        }
        Ok(Program {
            callables: found,
            by_key,
            places,
            types: Types::new(graph),
            texts,
            proof_checks: false,
        })
    }

    /// The same program, checking again every construction the checker
    /// proved follows its type's rules (`Type(value)`, an object's fields
    /// and a list written into a type with lengths): one that breaks a
    /// rule ends in a defect naming it. Compiled code never checks a proven
    /// construction, so a hole in a proof would pass an invalid value
    /// silently; tests run with this on to find one (chunk 2.7).
    pub fn with_proof_checks(mut self) -> Program {
        self.proof_checks = true;
        self
    }

    /// Every callable with a body, by key: functions, actions, fixtures,
    /// library functions, tests and seeds.
    pub fn keys(&self) -> impl Iterator<Item = &Key> + '_ {
        self.callables.iter().map(|callable| &callable.key)
    }

    /// Arguments to probe callable `name` with: for each parameter, values
    /// its type allows at and near the edges of its rules, combined, at
    /// most `limit` sets in all. `None` when a parameter's type isn't a
    /// number, text, a UUID or a flag, or a list or set of them, which
    /// this doesn't sample.
    pub fn probe_arguments(&self, name: &str, limit: usize) -> Option<Vec<Vec<Value>>> {
        let key = self.find(name)?;
        let callable = self.callable(self.id(key)?)?;
        let choices: Vec<Vec<Value>> = callable
            .body
            .params
            .iter()
            .map(|param| {
                let ty = &callable.body.locals.get(param.0 as usize)?.ty;
                let values = crate::probe::values(&self.types, ty)?;
                (!values.is_empty()).then_some(values)
            })
            .collect::<Option<_>>()?;
        Some(crate::probe::combine(&choices, limit))
    }

    /// Where every frame is.
    pub fn places(&self) -> &Places {
        &self.places
    }

    /// The callable a key's text or a name means: `function line_total`,
    /// `line_total`, `rounding.half_even`, or a test's key,
    /// `test main "a large size is rejected"`.
    pub fn find(&self, name: &str) -> Option<&Key> {
        self.callables
            .iter()
            .find(|callable| callable.key.as_str() == name)
            .or_else(|| {
                self.callables
                    .iter()
                    .find(|callable| callable.key.name() == name)
            })
            .map(|callable| &callable.key)
    }

    pub(crate) fn id(&self, key: &Key) -> Option<u32> {
        self.by_key.get(key).copied()
    }

    pub(crate) fn callable(&self, id: u32) -> Option<&Callable> {
        self.callables.get(id as usize)
    }

    /// Runs the callable `name` (see [`Program::find`]) with `args`, one
    /// per parameter, taken as values of the parameters' types: its value,
    /// the failure that left it, a fault, or a test's failed assertion.
    ///
    /// Each call is a tree walk on the thread's own stack, a few Rust frames
    /// per Jadpo frame; run deep recursion inside [`crate::on_large_stack`].
    pub fn call(&self, name: &str, args: Vec<Value>) -> Result<Outcome, CallError> {
        let key = self
            .find(name)
            .ok_or_else(|| CallError::Unknown(name.to_string()))?;
        let id = self
            .id(key)
            .ok_or_else(|| CallError::Unknown(name.to_string()))?;
        let callable = self
            .callable(id)
            .ok_or_else(|| CallError::Unknown(name.to_string()))?;
        if callable.body.params.len() != args.len() {
            return Err(CallError::Arity {
                callable: self.places.label(id).to_string(),
                expected: callable.body.params.len(),
                given: args.len(),
            });
        }
        Ok(Interp::run(self, id, args))
    }

    /// An object of type `ty` from its fields by name, in the order the
    /// type declares them; `None` when `ty` isn't an object or a field is
    /// missing or unknown.
    pub fn object(&self, ty: &str, fields: Vec<(&str, Value)>) -> Option<Value> {
        let key = [Kind::Type, Kind::Entity]
            .into_iter()
            .map(|kind| Key::new(kind, ty))
            .find(|key| self.types.graph().get(key).is_some())?;
        self.fields_by_name(&key, fields)
    }

    /// Variant `variant` of enum `ty`, with a payload variant's fields by
    /// name.
    pub fn variant(&self, ty: &str, variant: &str, fields: Vec<(&str, Value)>) -> Option<Value> {
        let key = Key::new(Kind::Variant, &format!("{ty}.{variant}"));
        self.types.graph().get(&key)?;
        self.fields_by_name(&key, fields)
    }

    fn fields_by_name(&self, key: &Key, fields: Vec<(&str, Value)>) -> Option<Value> {
        let shape = self.types.shape(key);
        if fields.len() != shape.fields.len() {
            return None;
        }
        let given = fields
            .into_iter()
            .map(|(name, value)| {
                shape
                    .fields
                    .iter()
                    .find(|field| &*field.name == name)
                    .map(|field| (field.key.clone(), value))
            })
            .collect::<Option<Vec<_>>>()?;
        self.types.build(key, given)
    }

    /// A value of the named type `ty` from `text`, as a literal of it would
    /// be: a UUID, an instant or a date from its ISO text, text otherwise.
    pub fn text_value(&self, ty: &str, text: &str) -> Option<Value> {
        let key = Key::new(Kind::Type, ty);
        let target = jadpo_graph::TypeRef::Node(key);
        let builtin = self.types.representation(&target)?;
        crate::eval::text_literal(builtin, text)
    }

    /// The report a person reads for an outcome that isn't a value.
    pub fn report(&self, outcome: &Outcome) -> Option<Report> {
        Report::of(self, outcome)
    }

    /// Where `frame` stands in its file; `None` for a callable's entry,
    /// which stands at no site.
    pub fn site(&self, frame: jadpo_runtime::Frame) -> Option<SiteSpan<'_>> {
        let callable = self.callable(frame.callable)?;
        let site = callable.body.sites.get(frame.site as usize)?;
        Some(SiteSpan {
            file: &callable.file,
            start: site.start,
            end: site.end,
            line: site.line,
        })
    }

    /// A file's text, by its project-relative path.
    pub fn text(&self, path: &str) -> Option<&str> {
        self.texts.get(path).map(String::as_str)
    }
}
