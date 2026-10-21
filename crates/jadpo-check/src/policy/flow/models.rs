//! What reaches a model (area 21, "Personal data sent to a model"; chunk
//! 5.5v5b): the walk again, following each prompt's placeholders and each
//! text `ai.embed` sends back to where they were read, so a marked field
//! copied into a `Text`, built into a string or a list, or passed through
//! a function keeps its category.
//!
//! The walk is the policy walk's ([`super`]), with two differences, each
//! the conservative one where it can't tell:
//!
//! - **Everything is followed:** a query's `Output` and a map layer, which
//!   the policy walk takes as proved where they are declared, are followed
//!   to their records' fields.
//! - **Types are sources too:** a value of a standard type that is
//!   personal data whatever holds it (area 19) is its category wherever it
//!   goes ([`Src::Typed`]), and a marked field read from a value the walk
//!   can't follow to a record is that field ([`Src::Marked`]).
//!
//! What decided a value is sent with it, as the policy walk counts it (a
//! condition, a `match`'s subject, a filter's predicate; Richard,
//! 2026-10-21), but for what a loop goes through: the records of a `for`'s
//! list, and `list.map`'s and `list.count`'s, decide nothing made from
//! them but what decided which records they are
//! ([`super::Walk::iterated`]), or a loop over records would send every
//! field of each.
//!
//! The sinks are each field of a `Prompt` built (whether `ai.generate`,
//! `ai.stream` or `prompts.render` takes it), checked with what its
//! declared type holds, and the text `ai.embed` and `ai.embed_all` send.
//! A parameter reaching one is checked where the action or function is
//! called ([`super::Summary::prompted`]), and where a subscriber passes
//! part of its event, against what every `emit` of the event put there
//! and the event's own marks. Each category reaching a model must be in
//! `ai: { personal_data: { allowed: } }`
//! (`semantic.ai_personal_data_not_allowed`), and a field marked
//! `to_models: false` reaches none (`semantic.ai_field_refused`).
//!
//! What a write stores is read later by whatever reads its field, a
//! prompt among them, so a field that stores a marked field's value, or
//! one it decided, has the same marks: each category and `to_models:
//! false` (`semantic.ai_field_unmarked`; Richard, 2026-10-21). A
//! parameter stored is checked where it is passed, and an event's part
//! where a subscriber passes it.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, File, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::TypeRef;
use jadpo_graph::body::{Body, SiteId};

use super::{Flow, Prov, Src, Walk, model_payloads, shallow};
use crate::ai_personal::{self, Holder, Holding};
use crate::data::Entities;
use crate::lookups::Lookup;
use crate::policy::proof::Prover;
use crate::types::{FieldsBase, Ty, TypeView};

/// The sink a value reaches: a prompt's placeholder, or `ai.embed`'s text.
const EMBED: &str = "ai.embed";

/// Where a value reaches a model, for a problem's words.
struct Reach<'a> {
    /// The prompt's type and its placeholder, `AnswerQuestion.question`,
    /// or [`EMBED`].
    sink: &'a str,
    /// The action or function it is passed to here, which sends it on.
    via: Option<&'a str>,
}

impl Reach<'_> {
    /// `` `AnswerQuestion`'s `{question}` ``, or the embedded text.
    fn sink(&self) -> String {
        match self.sink.rsplit_once('.') {
            _ if self.sink == EMBED => "the text `ai.embed` sends".to_string(),
            Some((prompt, placeholder)) => format!("`{prompt}`'s `{{{placeholder}}}`"),
            None => format!("`{}`", self.sink),
        }
    }
}

impl Walk<'_> {
    /// Marks `prov`, the value at `site`, with each standard type its type
    /// is that is personal data whatever holds it.
    pub(super) fn typed(&mut self, flow: &Flow<'_>, site: SiteId, prov: &mut Prov) {
        for (ty, category) in ai_personal::shallow(&flow.lookup, &self.body.site(site).ty) {
            prov.add(Src::Typed { ty, category });
        }
    }

    /// `receiver.field`, giving `out`: a marked field of an entity or an
    /// event, read from a value the walk can't follow to a record or a
    /// parameter (which its callers check), is that field.
    pub(super) fn marked(
        &mut self,
        flow: &Flow<'_>,
        receiver: SiteId,
        held: &Prov,
        field: &str,
        out: &mut Prov,
    ) {
        let mut ty = &self.body.site(receiver).ty;
        while let TypeRef::Nullable(inner) = ty {
            ty = inner;
        }
        let TypeRef::Node(key) = ty else {
            return;
        };
        let host = key.name();
        let followed = held
            .whole
            .iter()
            .any(|src| matches!(shallow(src.clone()), Src::Record { .. } | Src::Param(..)));
        if followed && flow.lookup.base(host) == Some(FieldsBase::Entity) {
            return;
        }
        if held
            .whole
            .iter()
            .any(|src| matches!(shallow(src.clone()), Src::Param(..)))
        {
            return;
        }
        let marks = ai_personal::marks(flow.db, flow.project);
        if ai_personal::field_holding(&flow.lookup, marks, host, field).is_some() {
            out.add(Src::Marked {
                host: host.to_string(),
                field: field.to_string(),
            });
        }
    }

    /// A `Prompt` built with `given`, each field's name, value and where
    /// it came from: each placeholder reaches the model.
    pub(super) fn prompt_built(
        &mut self,
        flow: &Flow<'_>,
        prompt: &str,
        given: &[(String, SiteId, Prov)],
    ) {
        if flow.lookup.base(prompt) != Some(FieldsBase::Prompt) {
            return;
        }
        let shown = prompt.rsplit('.').next().unwrap_or(prompt);
        for (name, value, prov) in given {
            let declared = flow
                .lookup
                .field(prompt, std::slice::from_ref(name))
                .map(|def| def.declared.clone());
            let sink = format!("{shown}.{name}");
            let span = self.span(*value);
            self.reaches(
                flow,
                span,
                prov,
                &Reach {
                    sink: &sink,
                    via: None,
                },
                declared.as_ref(),
            );
        }
    }

    /// `ai.embed`'s or `ai.embed_all`'s text, at `text`.
    pub(super) fn embedded(&mut self, flow: &Flow<'_>, text: SiteId, prov: &Prov) {
        let span = self.span(text);
        self.reaches(
            flow,
            span,
            prov,
            &Reach {
                sink: EMBED,
                via: None,
            },
            None,
        );
    }

    /// A call of `callee` with `args`, which sends the parts `prompted`
    /// names of what it is passed to a model.
    pub(super) fn passed_to_model(
        &mut self,
        flow: &Flow<'_>,
        callee: &str,
        prompted: &BTreeSet<(usize, Vec<String>, String)>,
        args: &[Option<SiteId>],
        provs: &[Prov],
    ) {
        for (at, path, sink) in prompted {
            let Some(Some(arg)) = args.get(*at) else {
                continue;
            };
            let prov = provs.get(*at).map(|p| p.at(path)).unwrap_or_default();
            let span = self.span(*arg);
            self.reaches(
                flow,
                span,
                &prov,
                &Reach {
                    sink,
                    via: Some(callee),
                },
                None,
            );
        }
    }

    /// `prov`, the value at `at`, reaches a model at `reach`, and with it
    /// whatever `declared`, the placeholder's type, holds: each holder of
    /// personal data in it checked against `allowed:`, and each parameter
    /// in it kept for the callers to check. What decided the value is
    /// sent with it (Richard, 2026-10-21).
    fn reaches(
        &mut self,
        flow: &Flow<'_>,
        at: Span,
        prov: &Prov,
        reach: &Reach<'_>,
        declared: Option<&Ty>,
    ) {
        if !self.report {
            return;
        }
        let mut held = holders(flow, prov, |param, path| {
            self.prompted.insert((param, path, reach.sink.to_string()));
        });
        if let Some(declared) = declared {
            let marks = ai_personal::marks(flow.db, flow.project);
            let mut found = BTreeMap::new();
            ai_personal::deep(
                &flow.lookup,
                marks,
                declared,
                &mut found,
                &mut BTreeSet::new(),
            );
            for (holder, holding) in found {
                held.hold(holder, &holding, false);
            }
        }
        held.fold_types();
        let allowed = &crate::ai::settings(flow.db, flow.project).allowed;
        for (holder, (holding, decided)) in held.0 {
            if !self.exposed.insert((at.start, at.end, holder.key())) {
                continue;
            }
            let how = How {
                holder: &holder,
                decided,
                reach,
            };
            let problem = if holding.refused {
                refused(&how, &at)
            } else {
                let unlisted: Vec<(&String, &Option<String>)> = holding
                    .categories
                    .iter()
                    .filter(|(category, _)| !allowed.contains(category))
                    .collect();
                if unlisted.is_empty() {
                    continue;
                }
                not_allowed(&how, &unlisted, &at)
            };
            self.exposures.push(problem);
        }
    }

    /// `prov`, given at `at`, stored in `entity`'s `field` (Richard,
    /// 2026-10-21): each holder of personal data in it, or that decided
    /// it, is one the field's own marks cover, each category it is and
    /// `to_models: false`, since whatever reads the field later, a prompt
    /// among them, sends what it holds; `callee` is the action it was
    /// passed to, which stores it, when it was.
    pub(super) fn stored_marked(
        &mut self,
        flow: &Flow<'_>,
        (entity, field): (&str, &str),
        at: Span,
        prov: &Prov,
        callee: Option<&str>,
    ) {
        if !self.report {
            return;
        }
        let marks = ai_personal::marks(flow.db, flow.project);
        let own =
            ai_personal::field_holding(&flow.lookup, marks, entity, field).unwrap_or_default();
        let mut held = holders(flow, prov, |_, _| {});
        held.fold_types();
        let into = format!("`{}.{field}`", entity.rsplit('.').next().unwrap_or(entity));
        for (holder, (holding, decided)) in held.0 {
            let missing: Vec<(&String, &Option<String>)> = holding
                .categories
                .iter()
                .filter(|(category, _)| !own.categories.contains_key(*category))
                .collect();
            let refused = holding.refused && !own.refused;
            if missing.is_empty() && !refused {
                continue;
            }
            if !self
                .exposed
                .insert((at.start, at.end, format!("{into} {}", holder.key())))
            {
                continue;
            }
            let problem = unmarked(
                &holder,
                decided,
                (&into, field),
                (&missing, refused),
                &at,
                callee.map(|callee| (self.name.as_str(), callee)),
            );
            self.exposures.push(problem);
        }
    }
}

/// The holders of personal data a value holds, each with what it is and
/// whether it only decided the value; a parameter's part in it is given
/// to `param`, for the callers to check.
struct Held(BTreeMap<Holder, (Holding, bool)>);

impl Held {
    fn hold(&mut self, holder: Holder, holding: &Holding, decided: bool) {
        let entry = self
            .0
            .entry(holder)
            .or_insert_with(|| (Holding::default(), true));
        for (category, from) in &holding.categories {
            entry
                .0
                .categories
                .entry(category.clone())
                .or_insert_with(|| from.clone());
        }
        entry.0.refused |= holding.refused;
        entry.1 &= decided;
    }

    /// A standard type a marked field is of is reported with the field.
    fn fold_types(&mut self) {
        let with_fields: BTreeSet<(String, String)> = self
            .0
            .iter()
            .filter(|(holder, _)| matches!(holder, Holder::Field { .. }))
            .flat_map(|(_, (holding, _))| holding.categories.iter())
            .filter_map(|(category, from)| Some((from.clone()?, category.clone())))
            .collect();
        self.0.retain(|holder, (holding, _)| match holder {
            Holder::Type(ty) => !holding
                .categories
                .keys()
                .all(|category| with_fields.contains(&(ty.clone(), category.clone()))),
            Holder::Field { .. } => true,
        });
    }
}

/// What `prov` holds that is personal data, and what decided it.
fn holders(flow: &Flow<'_>, prov: &Prov, mut param: impl FnMut(usize, Vec<String>)) -> Held {
    let marks = ai_personal::marks(flow.db, flow.project);
    let field =
        |host: &str, field: &str| ai_personal::field_holding(&flow.lookup, marks, host, field);
    let mut held = Held(BTreeMap::new());
    for src in prov.flat() {
        let (src, decided) = match shallow(src) {
            Src::Decided(inner) => (shallow(*inner), true),
            other => (other, false),
        };
        match src {
            Src::Field {
                entity,
                field: name,
                ..
            }
            | Src::Marked {
                host: entity,
                field: name,
            } => {
                if let Some(holding) = field(&entity, &name) {
                    held.hold(
                        Holder::Field {
                            host: entity,
                            field: name,
                        },
                        &holding,
                        decided,
                    );
                }
            }
            Src::Record { entity, given, .. } => {
                let names: Vec<String> = flow
                    .lookup
                    .fields(&entity)
                    .map(|fields| fields.iter().map(|f| f.name.clone()).collect())
                    .unwrap_or_default();
                for name in names.into_iter().filter(|name| !given.contains(name)) {
                    if let Some(holding) = field(&entity, &name) {
                        held.hold(
                            Holder::Field {
                                host: entity.clone(),
                                field: name,
                            },
                            &holding,
                            decided,
                        );
                    }
                }
            }
            Src::Typed { ty, category } => {
                let mut holding = Holding::default();
                holding.categories.insert(category, Some(ty.clone()));
                held.hold(Holder::Type(ty), &holding, decided);
            }
            Src::Param(at, path) => param(at, path),
            Src::Caller(_) | Src::Deep(_) | Src::Decided(_) => {}
        }
    }
    held
}

/// How a holder reaches the model: what it is, whether it only decided
/// the value, and where.
struct How<'a> {
    holder: &'a Holder,
    decided: bool,
    reach: &'a Reach<'a>,
}

/// The label at the value: what it holds or passes, or what decided it.
fn label(how: &How<'_>) -> String {
    match (how.decided, how.reach.via) {
        (true, _) => format!("decided by {}", how.holder.named()),
        (false, Some(_)) => format!("passes {}", how.holder.named()),
        (false, None) => format!("holds {}", how.holder.named()),
    }
}

/// How the value reaches the model, for a reason: `` `AnswerQuestion`'s
/// `{notes}` is built from `Patient.notes` ``.
fn how(how: &How<'_>) -> String {
    let sink = how.reach.sink();
    let holder = how.holder.named();
    match (how.reach.via, how.decided) {
        (Some(callee), false) => format!(
            "`{callee}` sends what it is passed here to a model, in {sink}, and this is {holder}"
        ),
        (Some(callee), true) => format!(
            "`{callee}` sends what it is passed here to a model, in {sink}, and {holder} decided it"
        ),
        (None, false) => format!("{sink} is built from {holder}"),
        (None, true) => format!("what {sink} holds is decided by {holder}"),
    }
}

/// Why what decided a value is sent with it, when it only decided it.
fn decides(how: &How<'_>) -> &'static str {
    if how.decided {
        " Which value is sent shows what decided it, so a value made under an `if` or a `match`, or kept by a filter, sends the fields they test."
    } else {
        ""
    }
}

fn refused(how: &How<'_>, at: &Span) -> Diagnostic {
    let holder = how.holder;
    Diagnostic::new(
        &rule::SEMANTIC_AI_FIELD_REFUSED,
        format!("{} reaches a model, and is `to_models: false`", holder.named()),
    )
    .at(at.clone(), label(how))
    .reason(format!(
        "`to_models: false` keeps a field out of every prompt and every text sent for an embedding, whatever `ai: {{ personal_data: {{ allowed: }} }}` lists; {}.{}",
        self::how(how),
        decides(how)
    ))
    .next(if how.decided {
        format!(
            "Decide {} by what isn't `to_models: false`",
            how.reach.sink()
        )
    } else {
        format!("Leave {} out of {}", holder.named(), how.reach.sink())
    })
}

fn not_allowed(how: &How<'_>, unlisted: &[(&String, &Option<String>)], at: &Span) -> Diagnostic {
    let holder = how.holder;
    let names: Vec<String> = unlisted
        .iter()
        .map(|(category, _)| format!("`{category}`"))
        .collect();
    let listed: Vec<String> = unlisted
        .iter()
        .map(|(category, _)| format!("`PersonalData.{category}`"))
        .collect();
    Diagnostic::new(
        &rule::SEMANTIC_AI_PERSONAL_DATA_NOT_ALLOWED,
        format!(
            "{} reaches a model, and `ai:` doesn't allow {} data",
            holder.named(),
            crate::and_list(&names)
        ),
    )
    .at(at.clone(), label(how))
    .reason(format!(
        "{}, {}. Each personal-data category sent to a model is listed in `ai: {{ personal_data: {{ allowed: }} }}`, so what the provider is given is decided in `app.jadpo`, where a reviewer sees it.{}",
        upper_first(&self::how(how)),
        crate::and_list(&what(holder, unlisted)),
        decides(how)
    ))
    .next(format!(
        "List {} in `ai: {{ personal_data: {{ allowed: }} }}`, or leave {} out of {}",
        crate::and_list(&listed),
        holder.named(),
        how.reach.sink()
    ))
}

/// What each category of `holder` is, for a reason: `` marked `personal:
/// PersonalData.health`, a sensitive category ``.
fn what(holder: &Holder, categories: &[(&String, &Option<String>)]) -> Vec<String> {
    categories
        .iter()
        .map(|(category, from)| {
            let sensitive = if ai_personal::is_sensitive(category) {
                ", a sensitive category"
            } else {
                ""
            };
            match (from, holder) {
                (None, _) => format!("marked `personal: PersonalData.{category}`{sensitive}"),
                (Some(_), Holder::Type(_)) => {
                    format!("`{category}` data whatever holds it{sensitive}")
                }
                (Some(ty), Holder::Field { .. }) => {
                    format!("a `{ty}`, `{category}` data whatever holds it{sensitive}")
                }
            }
        })
        .collect()
}

/// `semantic.ai_field_unmarked`: `holder`, or a value it decided,
/// stored in `into` (its `field`) without the marks `missing` names; with
/// the action and the callee it was passed to, when it was.
fn unmarked(
    holder: &Holder,
    decided: bool,
    (into, field): (&str, &str),
    (missing, refused): (&[(&String, &Option<String>)], bool),
    at: &Span,
    passed: Option<(&str, &str)>,
) -> Diagnostic {
    let named = holder.named();
    let mut lacks: Vec<String> = Vec::new();
    if !missing.is_empty() {
        let names: Vec<String> = missing
            .iter()
            .map(|(category, _)| format!("`{category}`"))
            .collect();
        lacks.push(format!("marked {}", crate::and_list(&names)));
    }
    if refused {
        lacks.push("`to_models: false`".to_string());
    }
    let lacks = crate::or_list(&lacks);
    let what_it_is = {
        let mut parts = what(holder, missing);
        if refused {
            parts.push("kept from every model with `to_models: false`".to_string());
        }
        crate::and_list(&parts)
    };
    let stored = if decided {
        format!("a value decided by {named}")
    } else {
        named.clone()
    };
    let headline = match passed {
        Some((action, callee)) => format!(
            "`{action}` passes {stored} to `{callee}`, which stores it in {into}, which isn't {lacks}"
        ),
        None => format!("{stored} is stored in {into}, which isn't {lacks}"),
    };
    let label = match (decided, passed) {
        (true, _) => format!("decided by {named}"),
        (false, Some(_)) => format!("passes {named}"),
        (false, None) => format!("holds {named}"),
    };
    let mut gives: Vec<String> = missing
        .iter()
        .map(|(category, _)| format!("`personal: PersonalData.{category}`"))
        .collect();
    if refused {
        gives.push("`to_models: false`".to_string());
    }
    let passes = passed
        .map(|(_, callee)| format!("`{callee}` stores what it is given here in {into}, and "))
        .unwrap_or_default();
    let shows = if decided {
        " Which value is stored shows what decided it, so a value made under an `if` or a `match`, or kept by a filter, keeps the marks of the fields they test."
    } else {
        ""
    };
    Diagnostic::new(&rule::SEMANTIC_AI_FIELD_UNMARKED, headline)
        .at(at.clone(), label)
        .reason(format!(
            "{passes}{named} is {what_it_is}. Whatever reads {into} later, a prompt among them, sends what it holds, so a field that stores a marked value keeps its marks: each category it is, in `personal:` or its type, and `to_models: false`.{shows}"
        ))
        .next(format!(
            "Give `{field}` {}, or leave {named} out of it",
            crate::and_list(&gives)
        ))
}

fn upper_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// What the subscribers in `file` pass a model from their events: each
/// part of its event a subscriber passes its action that the action
/// sends a model, or stores in a field without its marks, checked
/// against what every `emit` of the event put there and the event's own
/// marks.
pub(super) fn subscriber_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let reads = crate::events::subscribers_in(db, project, file);
    if reads.iter().all(|read| read.model.is_none()) {
        return Vec::new();
    }
    let flow = Flow {
        db,
        project,
        prover: Prover::new(db, project),
        lookup: Lookup::new(db, project),
        entities: Entities::new(db, project),
        personal: true,
    };
    let payloads = model_payloads(db, project);
    let marks = ai_personal::marks(db, project);
    let body = Body::default();
    let mut out = Vec::new();
    for read in reads {
        let Some(model) = &read.model else { continue };
        let summary = flow.summary(&model.run);
        if summary.prompted.is_empty() && summary.stores.is_empty() {
            continue;
        }
        let payload = payloads.get(&model.event).cloned().unwrap_or_default();
        let event = model.event.name();
        // The part of the event at `full`: what every `emit` of it put
        // there, and the event's own marks, each on its field.
        let marked = |name: &str| {
            ai_personal::field_holding(&flow.lookup, marks, event, name).map(|_| Src::Marked {
                host: event.to_string(),
                field: name.to_string(),
            })
        };
        let part = |full: &[String]| {
            let mut prov = payload.at(full);
            match full.first() {
                Some(first) => {
                    if let Some(src) = marked(first) {
                        prov.add(src);
                    }
                }
                None => {
                    for field in flow.lookup.fields(event).into_iter().flatten() {
                        if let Some(src) = marked(&field.name) {
                            prov.fields.entry(field.name.clone()).or_default().add(src);
                        }
                    }
                }
            }
            prov
        };
        let mut walk = Walk::blank(&body, file.path(db).clone(), model.name.clone());
        walk.report = true;
        walk.personal = true;
        for (at, path, sink) in &summary.prompted {
            let (Some(arg), Some(span)) = (model.args.get(*at), read.args.get(*at)) else {
                continue;
            };
            let mut full = arg.clone();
            full.extend(path.iter().cloned());
            walk.reaches(
                &flow,
                span.clone(),
                &part(&full),
                &Reach {
                    sink,
                    via: Some(model.run.name()),
                },
                None,
            );
        }
        let provs: Vec<Prov> = model.args.iter().map(|arg| part(arg)).collect();
        for store in &summary.stores {
            walk.event_store(&flow, model.run.name(), store, &read.args, &provs);
        }
        out.extend(walk.exposures);
    }
    out
}
