//! A born name is a name in use (area 18, "Stable identity"; area 14,
//! "`jadpo rename`"): a declaration or member written by hand with the name
//! another of its kind was born with would take that one's key, which
//! stored records and the review know it by, so `jadpo check` refuses it
//! as `jadpo rename` refuses the rename that would.

use std::collections::BTreeMap;

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::{Key, Node};

/// Each declaration or member whose key another of its kind already has
/// because that one was born with its name.
#[salsa::tracked(returns(ref))]
pub fn born_name_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    if crate::graph::registry(db, project).is_empty() {
        return Vec::new();
    }
    let mut by_key: BTreeMap<Key, Vec<Node>> = BTreeMap::new();
    let mut files: Vec<_> = project.files(db).to_vec();
    files.sort_by(|a, b| a.path(db).cmp(b.path(db)));
    for file in files {
        for node in &crate::graph::file_part(db, project, file).nodes {
            if crate::rename::renames(node.kind) && node.place.is_some() {
                by_key
                    .entry(node.key.clone())
                    .or_default()
                    .push(node.clone());
            }
        }
    }
    let own = |name: &str| name.rsplit('.').next().unwrap_or(name).to_string();
    let mut out = Vec::new();
    for (key, nodes) in by_key {
        if nodes.len() < 2 {
            continue;
        }
        let born = own(key.name());
        // The one written by hand has the born name as its own; the one
        // renamed has another. Two of the born name are a name declared
        // twice, which the naming check reports.
        let (written, renamed): (Vec<&Node>, Vec<&Node>) =
            nodes.iter().partition(|node| own(&node.name) == born);
        let (Some(renamed), false) = (renamed.first(), written.is_empty()) else {
            continue;
        };
        for node in written {
            out.push(clash(db, project, node, renamed, &born));
        }
    }
    out
}

fn clash(db: &dyn Db, project: Project, node: &Node, renamed: &Node, born: &str) -> Diagnostic {
    let word = node.kind.word().replace('_', " ");
    let mut problem = Diagnostic::new(
        &rule::SEMANTIC_DUPLICATE_DECLARATION,
        format!("`{born}` is the name `{}` was born with", renamed.name),
    )
    .reason(format!(
        "Stored records and the review know `{}` by the name it was born with, so a new {word} can't take `{born}` while `{}` keeps that identity.",
        renamed.name, renamed.name
    ))
    .next(format!("Choose another name for this {word}"));
    if let Some(span) = name_span(db, project, node, born) {
        problem = problem.at(span, format!("the name `{}` was born with", renamed.name));
    }
    problem
}

/// Where `name` is first written in `node`'s declaration.
fn name_span(db: &dyn Db, project: Project, node: &Node, name: &str) -> Option<Span> {
    let place = node.place.as_ref()?;
    let file = project
        .files(db)
        .iter()
        .find(|file| *file.path(db) == place.file)?;
    let text = file.text(db);
    let (start, end) = (place.start as usize, (place.end as usize).min(text.len()));
    let within = text.get(start..end)?;
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut from = 0;
    while let Some(at) = within[from..].find(name) {
        let begin = from + at;
        let after = begin + name.len();
        let before_ok = !within[..begin].chars().next_back().is_some_and(word);
        let after_ok = !within[after..].chars().next().is_some_and(word);
        if before_ok && after_ok {
            let begin = u32::try_from(start + begin).ok()?;
            let length = u32::try_from(name.len()).ok()?;
            return Some(Span::new(place.file.clone(), begin, begin + length));
        }
        from = after;
    }
    Some(Span::new(place.file.clone(), place.start, place.start))
}
