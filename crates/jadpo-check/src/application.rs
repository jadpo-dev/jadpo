//! What the root `application { }` holds (area 1, "What `application { }`
//! holds"; chunk A9): one list of every section an area page decides.
//! Each section is read by the check of the area that decides it; this
//! one refuses anything else, `semantic.application_section_unknown`, so
//! a misspelt setting meant to protect the app is never dropped without a
//! word, and refuses a section decided and not built yet,
//! `toolchain.not_built_yet`, where it is written rather than accepted and
//! ignored.

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{AstNode, MemberItem};

use crate::config::singletons;
use crate::tree::{self, node_span};

/// The sections `application { }` holds and this compiler builds, in the
/// order the error lists them, each read by its area's check.
pub const SECTIONS: [&str; 15] = [
    // Area 1: the Jadpo release line.
    "jadpo",
    // Area 8: the app's name, and how long a revoked caller may act.
    "display_name",
    "revocation_delay",
    // Area 11: every connection.
    "connections",
    // Area 14: the deployables.
    "deployables",
    // Area 7: the APIs and their versions.
    "apis",
    // Area 10: the client's address, and the root translations' review.
    "client_address",
    "translations",
    // Area 16: rate limits' store, and email.
    "rate_limits",
    "email",
    // Area 12: where events, schedules and steps are kept, and queues read.
    "events",
    "schedules",
    "steps",
    "receiving",
    // Area 21: the `ai` capability.
    "ai",
];

/// The sections an area page decides that this compiler doesn't build,
/// each with why accepting it would mislead.
pub const NOT_BUILT: [(&str, &str); 7] = [
    (
        "bot_check",
        "Checking a challenge token before a route or Jadpo's own public endpoints run is bot protection, which isn't built yet: accepted, every request would get through.",
    ),
    (
        "documents",
        "Turning the documents Jadpo renders into PDFs through a `documents` connection isn't built yet: accepted, no document would be made.",
    ),
    (
        "grants",
        "Encrypting the tokens of OAuth grants isn't built yet, nor the grants themselves: accepted, the key would guard nothing.",
    ),
    (
        "privacy",
        "The generated routes for a person's requests to export or erase their data aren't built yet: accepted, no request could be made.",
    ),
    (
        "publishing",
        "Delivering chosen events outward through a `publishing` connection isn't built yet: accepted, no event would leave the app.",
    ),
    (
        "scanning",
        "Scanning every upload through a `scanning` connection isn't built yet: accepted, files would be kept unscanned.",
    ),
    (
        "telemetry",
        "Sending traces, metrics and logs through `telemetry:` isn't built yet: accepted, nothing would be sent.",
    ),
];

/// The problems with the sections of each `application { }`.
#[salsa::tracked(returns(ref))]
pub fn section_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for found in singletons(db, project, "application") {
        for item in found.list.iter().flat_map(|list| list.member_items()) {
            let (written, span, respell) = match &item {
                MemberItem::Member(member) => {
                    let Some(key) = member.member_key() else {
                        continue;
                    };
                    let names: Vec<String> =
                        key.names().filter_map(|n| tree::name_text(&n)).collect();
                    // A dotted key is never respelt into one section.
                    let respell = names.len() == 1;
                    (
                        names.join("."),
                        node_span(&found.path, key.syntax()),
                        respell,
                    )
                }
                // `...` and a rule are refused where they are written.
                MemberItem::Elision(_) | MemberItem::RuleMember(_) => continue,
                // A bare name or a declaration is never a section,
                // whatever its first word.
                other => {
                    let span = node_span(&found.path, other.syntax());
                    out.push(unknown(span, &head(other.syntax()), false));
                    continue;
                }
            };
            if SECTIONS.contains(&written.as_str()) {
                continue;
            }
            match NOT_BUILT.iter().find(|(section, _)| *section == written) {
                Some((section, why)) => out.push(not_built(span, section, why)),
                None => out.push(unknown(span, &written, respell)),
            }
        }
    }
    out
}

fn not_built(span: Span, section: &str, why: &str) -> Diagnostic {
    Diagnostic::new(
        &rule::TOOLCHAIN_NOT_BUILT_YET,
        format!("The application's `{section}:` isn't built yet"),
    )
    .at(span, "decided, not built")
    .reason(why)
    .next(format!("Leave `{section}:` out for now"))
}

/// `written` refused, with the edit to the section it is nearest when
/// it is a section's name that can be respelt.
fn unknown(span: Span, written: &str, respell: bool) -> Diagnostic {
    let near = nearest(
        written,
        SECTIONS
            .iter()
            .copied()
            .chain(NOT_BUILT.iter().map(|(section, _)| *section)),
    )
    .filter(|_| respell);
    let list = crate::and_list(
        &SECTIONS
            .iter()
            .map(|s| format!("`{s}`"))
            .collect::<Vec<_>>(),
    );
    let mut reason = format!(
        "`application {{ }}` holds only the sections the language decides: {list}. A section it doesn't know would otherwise be dropped without a word."
    );
    if let Some(near) = near {
        reason.push_str(&format!(" Did you mean `{near}`?"));
    }
    let problem = Diagnostic::new(
        &rule::SEMANTIC_APPLICATION_SECTION_UNKNOWN,
        format!("`{written}` isn't something `application {{ }}` holds"),
    )
    .at(span.clone(), "not a section")
    .reason(reason);
    match near {
        Some(near) => problem
            .next(format!("Use `{near}` instead of `{written}`"))
            .edit(span, near),
        None => problem.next(format!(
            "Remove `{written}` from `application {{ }}`, or move it where it belongs"
        )),
    }
}

/// The first two words of an item that isn't `name: value`, as written.
fn head(node: &jadpo_syntax::SyntaxNode) -> String {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .take(2)
        .map(|token| token.text().to_string())
        .collect::<Vec<_>>()
        .join(" ")
}
