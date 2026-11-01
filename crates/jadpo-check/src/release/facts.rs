//! A release's facts (area 11, "What a release shows"): what the package
//! is, what it reaches and what it sends, as the same lines everywhere
//! they are shown. They cover what its code can do, so an operation of a
//! resource it keeps to itself counts as much as a public one: a hostile
//! release can't hide a host or a changing call behind a public action.
//! Which Jadpo releases it works with, and its last contract run, are the
//! registry's to add: they grow after the release is checked.

use jadpo_db::{Db, Project};
use jadpo_diag::rule;

use crate::ai_personal::Holder;
use crate::surface::{self, Surface, What};

/// A release's facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Facts {
    /// `library` or `provider`.
    pub kind: &'static str,
    /// A library package is pure by the rules it compiles under: no
    /// network, no entities, no routes.
    pub pure: bool,
    pub hosts: Vec<HostFact>,
    /// The operations that change something at the provider.
    pub changing_calls: Vec<String>,
    /// Each marked field, for each host it reaches.
    pub personal_data: Vec<PersonalDataFact>,
    /// Whether `provider.jadpo` declares an `erasure:` call.
    pub erasure: bool,
    /// Types an app may store: a library package's public types, and of a
    /// provider's only the plain values, never an object or an enum.
    pub storable_types: Vec<String>,
    /// Each public operation, action and function that can fail.
    pub failures: Vec<FailureFact>,
    /// How many operations it has, public or not.
    pub operations: usize,
    /// Those a recording confirms.
    pub confirmed_operations: Vec<String>,
    /// Where it could use a standard type and doesn't.
    pub standard_candidates: Vec<StandardCandidate>,
}

/// A host a package reaches, and the operations that reach it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostFact {
    pub host: String,
    pub operations: Vec<String>,
}

/// A personal-data field and a host it reaches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonalDataFact {
    /// `CustomerRequest.notes`, or a standard type sent whole.
    pub field: String,
    pub host: String,
    /// `customers.create`.
    pub operation: String,
    pub categories: Vec<String>,
    /// The value only decides what is sent, and isn't itself sent.
    pub decided_only: bool,
}

/// An operation, action or function and the failures it can give.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailureFact {
    pub declaration: String,
    pub failures: Vec<String>,
}

/// A field and the standard type it could use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandardCandidate {
    /// `Customer.created`.
    pub declaration: String,
    /// `Instant`.
    pub standard: String,
}

/// The facts of the package in `folder`; `None` when no package is there.
pub fn facts(db: &dyn Db, project: Project, folder: &str) -> Option<Facts> {
    let surface = surface::surface(db, project, folder)?;
    let operations = surface::all_operations(db, project, folder);
    let named = |wanted: fn(&What) -> bool| -> Vec<String> {
        operations
            .iter()
            .filter(|operation| wanted(&operation.what))
            .map(|operation| operation.name.clone())
            .collect()
    };
    let every: Vec<String> = named(|_| true);
    let hosts = reached(&surface)
        .into_iter()
        .map(|host| HostFact {
            host,
            operations: every.clone(),
        })
        .collect();
    let (to, sends) = crate::personal_data::package_sends(db, project, folder);
    let mut personal_data = Vec::new();
    for send in &sends {
        for host in &to {
            personal_data.push(PersonalDataFact {
                field: named_holder(&send.holder),
                host: host.clone(),
                operation: send.operation.clone(),
                categories: send.categories.clone(),
                decided_only: send.decided,
            });
        }
    }
    let library = surface.provider.is_none();
    Some(Facts {
        kind: surface.kind,
        pure: library,
        hosts,
        changing_calls: named(|what| matches!(what, What::Operation { changes: true, .. })),
        personal_data,
        erasure: surface.provider.as_ref().is_some_and(|p| p.erasure),
        storable_types: storable(&surface),
        failures: failures(&surface),
        operations: every.len(),
        confirmed_operations: named(|what| {
            matches!(
                what,
                What::Operation {
                    confirmed: Some(_),
                    ..
                }
            )
        }),
        standard_candidates: standard_candidates(db, project, folder),
    })
}

/// What a package sends, as its facts and `jadpo package check` name it:
/// `CustomerRequest.notes`, without the package's own name.
pub fn named_holder(holder: &Holder) -> String {
    match holder {
        Holder::Field { host, field } => {
            format!("{}.{field}", host.rsplit('.').next().unwrap_or(host))
        }
        Holder::Type(ty) => ty.clone(),
    }
}

/// Every host the provider's entry names, or what stands for one when it
/// names none.
fn reached(surface: &Surface) -> Vec<String> {
    let Some(provider) = &surface.provider else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for fact in &provider.facts {
        if let Some(rest) = fact.strip_prefix("host ") {
            if let Some((_, host)) = rest.split_once(' ') {
                out.push(host.to_string());
            }
        } else if let Some(host) = fact.strip_prefix("runs asks ") {
            out.push(host.to_string());
        } else if let Some(image) = fact.strip_prefix("runs image ") {
            out.push(format!("the service Jadpo runs from {image}"));
        }
    }
    if out.is_empty() {
        out.push("the address its connection gives".to_string());
    }
    out
}

/// The public types an app may keep (area 11, "What can be stored").
fn storable(surface: &Surface) -> Vec<String> {
    let library = surface.provider.is_none();
    let mut out: Vec<String> = surface
        .declarations
        .iter()
        .filter(|declaration| match &declaration.what {
            What::Type { base, .. } => {
                library || !matches!(base.as_str(), "Object" | "Enum" | "Input" | "Output")
            }
            _ => false,
        })
        .map(|declaration| declaration.name.clone())
        .collect();
    out.sort();
    out
}

fn failures(surface: &Surface) -> Vec<FailureFact> {
    surface
        .declarations
        .iter()
        .filter_map(|declaration| {
            let fails = match &declaration.what {
                What::Operation { fails, .. }
                | What::Action { fails, .. }
                | What::Function { fails, .. } => fails,
                _ => return None,
            };
            (!fails.is_empty()).then(|| FailureFact {
                declaration: declaration.name.clone(),
                failures: fails.clone(),
            })
        })
        .collect()
}

/// `package.standard_candidate`'s findings on the package's types, declined
/// or not: a package that compiles has declined each, and the fact shows
/// what it declined. The finding names the field first and the standard
/// type last (`` `Charge.created` may be an `Instant` sent as seconds ``,
/// `` `Stop.lat` and `Stop.lng` may be a `Point` ``).
fn standard_candidates(db: &dyn Db, project: Project, folder: &str) -> Vec<StandardCandidate> {
    let prefix = format!("{folder}/");
    let mut out = Vec::new();
    for &file in project.files(db) {
        let path = file.path(db);
        if !path.starts_with(&prefix) || path.ends_with(".test.jadpo") {
            continue;
        }
        for finding in crate::types::type_problems(db, project, file) {
            if finding.rule.id != rule::PACKAGE_STANDARD_CANDIDATE.id {
                continue;
            }
            let quoted: Vec<&str> = finding.message.split('`').skip(1).step_by(2).collect();
            if let (Some(field), Some(standard)) = (quoted.first(), quoted.last()) {
                out.push(StandardCandidate {
                    declaration: (*field).to_string(),
                    standard: (*standard).to_string(),
                });
            }
        }
    }
    out.sort_by(|a, b| a.declaration.cmp(&b.declaration));
    out
}
