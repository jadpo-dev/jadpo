//! Checking a Jadpo project (docs/compiler-runtime.md §2.2): the rules of
//! each language area as salsa queries over `jadpo-db`. Area 1 is built:
//! layout, naming, visibility and resolution. Later chunks add types,
//! logic and failures (areas 2 to 4) and build the graph.
//!
//! [`check`] assembles every diagnostic for a project, sorted by file and
//! position so the same source always gives the same output.

mod case;
mod layout;
mod naming;
mod resolve;
mod std_names;
mod tree;
mod usage;

pub use layout::layout_problems;
pub use naming::{domain_problems, naming_problems};
pub use resolve::resolve_file;
pub use usage::{mentions, usage_problems, users};

use jadpo_db::{Db, Project, item_tree, parse};
use jadpo_diag::{Diagnostic, rule};

/// Every problem in `project`, in a stable order. Syntax errors come alone:
/// a file that does not parse would only add knock-on errors.
pub fn check(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let files = project.files(db);
    let mut out: Vec<Diagnostic> = files
        .iter()
        .flat_map(|file| {
            let path = file.path(db);
            parse(db, *file)
                .errors()
                .iter()
                .map(|error| error.to_diagnostic(path))
                .collect::<Vec<_>>()
        })
        .collect();
    if out.is_empty() {
        if files
            .iter()
            .all(|file| item_tree(db, *file).items.is_empty())
        {
            out.push(empty_project(files.len()));
        }
        out.extend(domain_problems(db, project).iter().cloned());
        for &file in files {
            out.extend(layout_problems(db, project, file).iter().cloned());
            out.extend(naming_problems(db, project, file).iter().cloned());
            out.extend(resolve_file(db, project, file).iter().cloned());
            out.extend(usage_problems(db, project, file).iter().cloned());
        }
    }
    sort(&mut out);
    out
}

/// By file, then position, then rule, so output never depends on the
/// order checks ran in; repeats are dropped.
fn sort(diagnostics: &mut Vec<Diagnostic>) {
    let key = |d: &Diagnostic| {
        let (path, start, end) = d.primary.as_ref().map_or((String::new(), 0, 0), |label| {
            (label.span.path.clone(), label.span.start, label.span.end)
        });
        (path, start, end, d.rule.id, d.message.clone())
    };
    diagnostics.sort_by_key(key);
    diagnostics.dedup();
}

fn empty_project(files: usize) -> Diagnostic {
    let reason = match files {
        0 => "Jadpo found no `.jadpo` files.".to_string(),
        1 => "Its one `.jadpo` file holds no declarations, only comments or nothing at all."
            .to_string(),
        n => {
            format!("Its {n} `.jadpo` files hold no declarations, only comments or nothing at all.")
        }
    };
    Diagnostic::new(
        &rule::TOOLCHAIN_EMPTY_PROJECT,
        "This project declares nothing",
    )
    .reason(reason)
    .next("Add a declaration, such as a `type`, `function` or `action`")
}

pub(crate) fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}
