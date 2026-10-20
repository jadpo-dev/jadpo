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
//! What decided a value (a condition, a loop's list, a filter's
//! predicate) isn't sent with it: a placeholder carries the fields it was
//! read or built from, as the page says, and a loop over records would
//! otherwise send every field of each.
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
    /// in it kept for the callers to check.
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
        let marks = ai_personal::marks(flow.db, flow.project);
        let mut held: BTreeMap<Holder, Holding> = BTreeMap::new();
        let mut hold = |holder: Holder, holding: &Holding| {
            let entry = held.entry(holder).or_default();
            for (category, from) in &holding.categories {
                entry
                    .categories
                    .entry(category.clone())
                    .or_insert_with(|| from.clone());
            }
            entry.refused |= holding.refused;
        };
        let field =
            |host: &str, field: &str| ai_personal::field_holding(&flow.lookup, marks, host, field);
        for src in prov.flat() {
            // What decided the value isn't sent with it.
            match shallow(src) {
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
                        hold(
                            Holder::Field {
                                host: entity,
                                field: name,
                            },
                            &holding,
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
                            hold(
                                Holder::Field {
                                    host: entity.clone(),
                                    field: name,
                                },
                                &holding,
                            );
                        }
                    }
                }
                Src::Typed { ty, category } => {
                    let mut holding = Holding::default();
                    holding.categories.insert(category, Some(ty.clone()));
                    hold(Holder::Type(ty), &holding);
                }
                Src::Param(param, path) => {
                    self.prompted.insert((param, path, reach.sink.to_string()));
                }
                Src::Caller(_) | Src::Deep(_) | Src::Decided(_) => {}
            }
        }
        if let Some(declared) = declared {
            let mut found = BTreeMap::new();
            ai_personal::deep(
                &flow.lookup,
                marks,
                declared,
                &mut found,
                &mut BTreeSet::new(),
            );
            for (holder, holding) in found {
                hold(holder, &holding);
            }
        }
        // A standard type a marked field is of is reported with the field.
        let with_fields: BTreeSet<(String, String)> = held
            .iter()
            .filter(|(holder, _)| matches!(holder, Holder::Field { .. }))
            .flat_map(|(_, holding)| holding.categories.iter())
            .filter_map(|(category, from)| Some((from.clone()?, category.clone())))
            .collect();
        held.retain(|holder, holding| match holder {
            Holder::Type(ty) => !holding
                .categories
                .keys()
                .all(|category| with_fields.contains(&(ty.clone(), category.clone()))),
            Holder::Field { .. } => true,
        });
        let allowed = &crate::ai::settings(flow.db, flow.project).allowed;
        for (holder, holding) in held {
            if !self.exposed.insert((at.start, at.end, holder.key())) {
                continue;
            }
            let problem = if holding.refused {
                refused(&holder, &at, reach)
            } else {
                let unlisted: Vec<(&String, &Option<String>)> = holding
                    .categories
                    .iter()
                    .filter(|(category, _)| !allowed.contains(category))
                    .collect();
                if unlisted.is_empty() {
                    continue;
                }
                not_allowed(&holder, &unlisted, &at, reach)
            };
            self.exposures.push(problem);
        }
    }
}

/// The label at the value: what it holds or passes.
fn label(holder: &Holder, reach: &Reach<'_>) -> String {
    match reach.via {
        Some(_) => format!("passes {}", holder.named()),
        None => format!("holds {}", holder.named()),
    }
}

/// How the value reaches the model, for a reason: `` `AnswerQuestion`'s
/// `{notes}` is built from `Patient.notes` ``.
fn how(holder: &Holder, reach: &Reach<'_>) -> String {
    let sink = reach.sink();
    match reach.via {
        Some(callee) => format!(
            "`{callee}` sends what it is passed here to a model, in {sink}, and this is {}",
            holder.named()
        ),
        None => format!("{sink} is built from {}", holder.named()),
    }
}

fn refused(holder: &Holder, at: &Span, reach: &Reach<'_>) -> Diagnostic {
    Diagnostic::new(
        &rule::SEMANTIC_AI_FIELD_REFUSED,
        format!("{} reaches a model, and is `to_models: false`", holder.named()),
    )
    .at(at.clone(), label(holder, reach))
    .reason(format!(
        "`to_models: false` keeps a field out of every prompt and every text sent for an embedding, whatever `ai: {{ personal_data: {{ allowed: }} }}` lists; {}.",
        how(holder, reach)
    ))
    .next(format!("Leave {} out of {}", holder.named(), reach.sink()))
}

fn not_allowed(
    holder: &Holder,
    unlisted: &[(&String, &Option<String>)],
    at: &Span,
    reach: &Reach<'_>,
) -> Diagnostic {
    let names: Vec<String> = unlisted
        .iter()
        .map(|(category, _)| format!("`{category}`"))
        .collect();
    let what: Vec<String> = unlisted
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
    .at(at.clone(), label(holder, reach))
    .reason(format!(
        "{}, {}. Each personal-data category sent to a model is listed in `ai: {{ personal_data: {{ allowed: }} }}`, so what the provider is given is decided in `app.jadpo`, where a reviewer sees it.",
        upper_first(&how(holder, reach)),
        crate::and_list(&what)
    ))
    .next(format!(
        "List {} in `ai: {{ personal_data: {{ allowed: }} }}`, or leave {} out of {}",
        crate::and_list(&listed),
        holder.named(),
        reach.sink()
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
/// sends a model, checked against what every `emit` of the event put
/// there and the event's own marks.
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
        let prompted = &flow.summary(&model.run).prompted;
        if prompted.is_empty() {
            continue;
        }
        let payload = payloads.get(&model.event).cloned().unwrap_or_default();
        let event = model.event.name();
        let mut walk = Walk::blank(&body, file.path(db).clone(), model.name.clone());
        walk.report = true;
        walk.personal = true;
        for (at, path, sink) in prompted {
            let (Some(arg), Some(span)) = (model.args.get(*at), read.args.get(*at)) else {
                continue;
            };
            let mut full = arg.clone();
            full.extend(path.iter().cloned());
            let mut prov = payload.at(&full);
            // The event's own fields, as it declares them.
            let fields: Vec<String> = match full.first() {
                Some(first) => vec![first.clone()],
                None => flow
                    .lookup
                    .fields(event)
                    .map(|fields| fields.iter().map(|f| f.name.clone()).collect())
                    .unwrap_or_default(),
            };
            for name in fields {
                if ai_personal::field_holding(&flow.lookup, marks, event, &name).is_some() {
                    prov.add(Src::Marked {
                        host: event.to_string(),
                        field: name,
                    });
                }
            }
            walk.reaches(
                &flow,
                span.clone(),
                &prov,
                &Reach {
                    sink,
                    via: Some(model.run.name()),
                },
                None,
            );
        }
        out.extend(walk.exposures);
    }
    out
}
