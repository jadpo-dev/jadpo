//! Moving a domain (area 14, "Tools for agents"; area 1, "Domains"; area
//! 18, "Stable identity"): `jadpo domain rename`, `split` and `merge` run
//! this code, as `jadpo rename` runs [`crate::rename`], whose steps it
//! shares.
//!
//! - **Rename** moves the domain's folder and rewrites every place code
//!   names the domain: its `use` lines, the calls made through it
//!   (`planning.add_todo(...)`), its lines in `policy { actions: }` and the
//!   settings that list it (`domains: [planning]`). The identity registry
//!   gets one line, `domain planning -> plans`, which the domain's types,
//!   functions and constants follow, since their keys carry it.
//! - **Merge** moves every file of one domain to the same place in
//!   another. A call into either from the other loses its qualifier, since
//!   code calls its own domain by bare name; a call from elsewhere names
//!   the domain it merged into. Each type, function and constant whose key
//!   carries the domain gets a registry line keeping the key it was born
//!   with. Two files at one place, or a private name declared in both, is
//!   refused before anything changes.
//! - **Split** moves the declarations named, with the tests beside them
//!   and their templates and prompts, to a new domain, along a seam: no
//!   reference may cross between what moves and what stays, so the
//!   refusal names each one that does and, when what they reach together
//!   leaves something behind, the command that moves all of it. Calls
//!   into what moved name the new domain, and a setting that lists the
//!   domain lists the new one beside it.
//! - **Imports** are left to [`crate::fix_imports`], which moves each name
//!   to the folder it is in now and drops a `use` line nothing needs, so
//!   the moved code imports as if it had been written where it now is.
//! - **Records** the move puts out of date (the API, event, access and
//!   network records) are written again; a domain's own generated records
//!   move with it, or, merged, are written again for the domain they join.
//!   `jadpo check` judges whether the change needs an acceptance:
//!   `DomainChange.renamed` for a rename, `DomainChange.created` for the
//!   domain a split makes.
//! - **Stopped**: a result with an error the project didn't have, such as
//!   a threshold left watching an action that moved, is undone and shown.
//!
//! [`plan`] reads and decides, editing nothing; [`apply`] edits the loaded
//! project and checks the result; the command writes what it gives back.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, Loaded, Project, Symbol};
use jadpo_diag::{Diagnostic, nearest, rule};
use jadpo_graph::{Graph, Key, Kind, Node, Registry};
use jadpo_syntax::SyntaxKind::{DOT, IDENT, USE_DECL};
use jadpo_syntax::ast::AstNode;

use crate::case::is_lower_snake;
use crate::graph::{Found, lookup, node_references};
use crate::rename::{added_errors, format_as_before, headlines, record_states, rewrite_records};
use crate::setting_move::{FileChange, Mark, Row, Stopped};
use crate::std_lib::StdNames;

/// What to do with a domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Move {
    /// Give `domain` the name `to`.
    Rename { domain: String, to: String },
    /// Move every file of `domain` into `into`.
    Merge { domain: String, into: String },
    /// Move the declarations `names` of `domain` to a new domain, `to`.
    Split {
        domain: String,
        to: String,
        names: Vec<String>,
    },
}

impl Move {
    /// The command's word: `rename`, `merge` or `split`.
    pub fn word(&self) -> &'static str {
        match self {
            Move::Rename { .. } => "rename",
            Move::Merge { .. } => "merge",
            Move::Split { .. } => "split",
        }
    }

    pub fn domain(&self) -> &str {
        match self {
            Move::Rename { domain, .. }
            | Move::Merge { domain, .. }
            | Move::Split { domain, .. } => domain,
        }
    }

    /// Where it goes: the new name, the domain it joins, or the new domain.
    pub fn to(&self) -> &str {
        match self {
            Move::Rename { to, .. } | Move::Split { to, .. } => to,
            Move::Merge { into, .. } => into,
        }
    }
}

/// One text replacement, with its line in the text before.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Edit {
    start: u32,
    end: u32,
    replacement: String,
    line: u32,
    /// What the change list says of it; `None` for a `use` line's, which
    /// the diff shows.
    note: Option<String>,
}

/// What a file is to the project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Held {
    Source,
    Record,
    Other,
}

/// What [`plan`] decided.
#[derive(Clone, Debug)]
pub struct Plan {
    pub what: Move,
    /// Each file that moves, from its path to its path after, and what it
    /// is.
    moves: Vec<(String, String, Held)>,
    /// The domain's generated records that a merge leaves to be written
    /// again for the domain it joins.
    dropped: Vec<String>,
    /// Each file's replacements, by its path before the move.
    edits: BTreeMap<String, Vec<Edit>>,
    /// Every file that names the domain, by its path before the move: its
    /// imports are fixed after it.
    touched: BTreeSet<String>,
    /// The registry's lines after, and the rows saying what changed.
    lines: BTreeMap<String, String>,
    line_rows: Vec<Row>,
    /// The stored tables that stay where they are: `table todo`.
    pub kept: Vec<String>,
    /// For a split: the declarations that move, and how many stay.
    pub moved: Vec<String>,
    pub stayed: usize,
}

/// What [`apply`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moved {
    /// Every file it changed, to write: the moved files' old paths with no
    /// text after, their new paths with no text before.
    pub files: Vec<FileChange>,
    /// The change list, in the order it is shown: each group's path and
    /// rows.
    pub groups: Vec<(String, Vec<Row>)>,
    /// Files neither source nor a record that move, by path before and
    /// after: the command moves them as they are on disk.
    pub others: Vec<(String, String)>,
    /// The errors the project still has, every one of which it had before.
    pub errors_left: usize,
    /// Whether a record other than the registry was written again.
    pub records: bool,
}

/// `cli.domain_arguments`, with its reason and next step.
fn refused(message: String, reason: String, next: String) -> Box<Diagnostic> {
    Box::new(
        Diagnostic::new(&rule::CLI_DOMAIN_ARGUMENTS, message)
            .reason(reason)
            .next(next),
    )
}

/// `a`, `a and b`, `a, b and c`, each in backticks.
fn listed(items: &[String]) -> String {
    let ticked: Vec<String> = items.iter().map(|item| format!("`{item}`")).collect();
    crate::and_list(&ticked)
}

/// The folder of domain `domain`, with its slash.
fn folder(domain: &str) -> String {
    format!("domains/{domain}/")
}

/// Every file the project holds under domain `domain`'s folder, in path
/// order, with what it is.
fn files_of(db: &dyn Db, project: Project, domain: &str) -> Vec<(String, Held)> {
    let prefix = folder(domain);
    let mut out: Vec<(String, Held)> = project
        .files(db)
        .iter()
        .map(|file| (file.path(db).clone(), Held::Source))
        .chain(
            project
                .records(db)
                .iter()
                .map(|record| (record.path.clone(), Held::Record)),
        )
        .chain(
            project
                .other_files(db)
                .iter()
                .map(|path| (path.clone(), Held::Other)),
        )
        .filter(|(path, _)| path.starts_with(&prefix))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Whether the record at `path` is one Jadpo writes from a domain's code:
/// its event record or its translations' sources.
fn generated(path: &str) -> bool {
    jadpo_db::record::kind_of(path)
        .is_some_and(|(kind, _)| matches!(kind.word, "events" | "translations"))
}

/// The domain's node key.
fn domain_key(registry: &Registry, domain: &str) -> Key {
    registry.key(Kind::Domain, domain, None)
}

/// `cli.domain_arguments` for a domain the project doesn't have, with the
/// command again for the nearest.
fn unknown_domain(
    domains: &BTreeSet<String>,
    name: &str,
    again: impl Fn(&str) -> String,
) -> Box<Diagnostic> {
    let all: Vec<String> = domains.iter().cloned().collect();
    let near = nearest(name, all.iter().map(String::as_str));
    let reason = match near {
        Some(near) => format!(
            "This project's domains are {}. Did you mean `{near}`?",
            listed(&all)
        ),
        None if all.is_empty() => "This project has no domains.".to_string(),
        None => format!("This project's domains are {}.", listed(&all)),
    };
    let next = match near {
        Some(near) => format!("Run `{}`", again(near)),
        None => "Run `jadpo inspect` to list the project's domains".to_string(),
    };
    refused(format!("`{name}` isn't a domain"), reason, next)
}

/// Why `name` can't be a new domain's name, if it can't: `again` is the
/// command to run with another.
fn name_refused(db: &dyn Db, project: Project, name: &str, again: &str) -> Option<Box<Diagnostic>> {
    let names = StdNames::get();
    if !is_lower_snake(name) {
        let example = crate::case::to_snake(name);
        let example = if is_lower_snake(&example) {
            format!(", as `{example}` is")
        } else {
            String::new()
        };
        return Some(refused(
            format!("`{name}` isn't a name for a domain"),
            format!(
                "A domain's name is its folder's, written in lower_snake_case before each call into it{example}."
            ),
            format!("Run `{again}` with a lower_snake_case name"),
        ));
    }
    if crate::naming::is_reserved_word(name) {
        return Some(refused(
            format!("`{name}` is a reserved word and can't name a domain"),
            format!(
                "A domain's name is written before each call into it, and `{name}.` would read as the word, not the domain."
            ),
            format!("Run `{again}` with another name"),
        ));
    }
    if names.reserved(name) {
        return Some(refused(
            format!("`{name}` is a standard-library namespace"),
            "Standard-library namespaces can't name a domain.".to_string(),
            format!("Run `{again}` with another name"),
        ));
    }
    if names.is_generic(name) {
        return Some(refused(
            format!("`{name}` is too generic for a domain"),
            "A domain is named after the business area it covers, such as `ordering`, so a reader knows what is in it.".to_string(),
            format!("Run `{again}` with the name of a business area"),
        ));
    }
    let symbols = jadpo_db::symbols(db, project);
    if let Some(taken) = symbols.declarations.get(name).and_then(|found| {
        found
            .iter()
            .find(|symbol| symbol.provided.is_none() && symbol.ownership.package().is_none())
    }) {
        return Some(refused(
            format!("`{name}` is already declared"),
            format!(
                "`{name}` is the {} in `{}`, and a domain's name is written before each call into it, so the two can't share it.",
                taken.kind.word(),
                taken.path
            ),
            format!("Choose another name, then run `{again}`"),
        ));
    }
    if symbols.connections.contains(name) || symbols.packages.contains(name) {
        return Some(refused(
            format!("`{name}` is already a connection's or a package's name"),
            "A domain's name is written before each call into it, as a connection's and a package's are, so the two can't share it.".to_string(),
            format!("Choose another name, then run `{again}`"),
        ));
    }
    None
}

/// Whether `node` is a type, function or constant whose key carries domain
/// `domain` (area 18, "Keys"), named `<domain>.<name>` in the registry.
fn carries_domain(graph: &Graph, registry: &Registry, node: &Node, domain: &str) -> bool {
    matches!(node.kind, Kind::Type | Kind::Function | Kind::Constant)
        && node.domain.as_deref() == Some(domain)
        && node
            .parent
            .as_ref()
            .and_then(|parent| graph.get(parent))
            .is_none_or(|parent| parent.kind != Kind::Library)
        && registry.key(node.kind, &format!("{domain}.{}", node.name), None) == node.key
}

/// The registry's kinds whose name now starts with the domain's: a type's,
/// a function's or a constant's, and their members'.
const CARRIED: &[&str] = &["type", "function", "constant", "field", "variant", "rule"];

/// The registry's lines after `nodes`, each carrying domain `from`, take
/// domain `to` instead; with the rows that say so.
fn carried_lines(
    lines: &mut BTreeMap<String, String>,
    nodes: &[&Node],
    from: &str,
    to: &str,
    rows: &mut Vec<Row>,
) {
    for node in nodes {
        let word = node.kind.word();
        let now = format!("{from}.{}", node.name);
        let after = format!("{to}.{}", node.name);
        let born = node.key.name().to_string();
        let had = lines.remove(&format!("{word} {now}"));
        // Its members' lines follow it.
        let prefix = format!("{now}.");
        let members: Vec<(String, String)> = lines
            .iter()
            .filter(|(line, _)| {
                line.split_once(' ').is_some_and(|(kind, name)| {
                    matches!(kind, "field" | "variant" | "rule") && name.starts_with(&prefix)
                })
            })
            .map(|(line, born)| (line.clone(), born.clone()))
            .collect();
        for (line, born) in members {
            lines.remove(&line);
            let (kind, name) = line.split_once(' ').unwrap_or_default();
            lines.insert(format!("{kind} {after}.{}", &name[prefix.len()..]), born);
        }
        let needed = Registry::from_lines(&*lines).born(node.kind, &after, None) != born;
        if needed {
            lines.insert(format!("{word} {after}"), born.clone());
        }
        let subject = format!("{word} {born}");
        rows.push(match (had.is_some(), needed) {
            (false, true) => Row {
                mark: Mark::Added,
                subject,
                note: format!("now {after}"),
            },
            (true, true) => Row {
                mark: Mark::Changed,
                subject,
                note: format!("now {after}, was {now}"),
            },
            (true, false) => Row {
                mark: Mark::Removed,
                subject,
                note: "back to its born name".to_string(),
            },
            (false, false) => continue,
        });
    }
}

/// One place a domain is written, read from its file's tree.
struct Site {
    path: String,
    start: u32,
    end: u32,
    line: u32,
    /// On a `use` line.
    in_use: bool,
    /// When a `.` follows it: where the dot ends, and the name after it
    /// with what that name refers to.
    qualifies: Option<(u32, String, Option<Key>)>,
}

/// Every place domain `key` is written, outside migration folders and
/// installed packages, each file's path kept in `touched`.
fn sites(db: &dyn Db, project: Project, key: &Key, touched: &mut BTreeSet<String>) -> Vec<Site> {
    let mut out = Vec::new();
    for reference in node_references(db, project, key) {
        if reference.path.starts_with("database/") {
            continue;
        }
        let Some(file) = project
            .files(db)
            .iter()
            .copied()
            .find(|file| *file.path(db) == reference.path)
        else {
            continue;
        };
        if jadpo_db::location(db, project, file)
            .ownership
            .package()
            .is_some()
        {
            continue;
        }
        let Some(source) = crate::tree::source(db, file) else {
            continue;
        };
        let Some(token) = source
            .syntax()
            .token_at_offset(reference.start.into())
            .right_biased()
        else {
            continue;
        };
        let in_use = token.parent_ancestors().any(|node| node.kind() == USE_DECL);
        let next_real = |token: &jadpo_syntax::SyntaxToken| {
            let mut next = token.next_token();
            while let Some(candidate) = &next
                && candidate.kind().is_trivia()
            {
                next = candidate.next_token();
            }
            next
        };
        let qualifies = next_real(&token)
            .filter(|dot| dot.kind() == DOT)
            .and_then(|dot| {
                let name = next_real(&dot).filter(|name| name.kind() == IDENT)?;
                let at = u32::from(name.text_range().start());
                let refers = match lookup(db, project, file, at) {
                    Some((Found::Node(key), _)) => Some(key),
                    _ => None,
                };
                Some((
                    u32::from(dot.text_range().end()),
                    name.text().to_string(),
                    refers,
                ))
            });
        let text = file.text(db);
        touched.insert(reference.path.clone());
        out.push(Site {
            path: reference.path.clone(),
            start: reference.start,
            end: reference.end,
            line: text[..reference.start as usize].matches('\n').count() as u32 + 1,
            in_use,
            qualifies,
        });
    }
    out
}

/// The domain's top-level declarations as written, not the failures the
/// compiler provides for them: what a split names and counts.
fn declarations<'g>(
    db: &dyn Db,
    project: Project,
    graph: &'g Graph,
    domain: &str,
) -> Vec<&'g Node> {
    let symbols = jadpo_db::symbols(db, project);
    graph
        .nodes()
        .iter()
        .filter(|node| {
            node.domain.as_deref() == Some(domain)
                && node.parent.is_none()
                && !matches!(
                    node.kind,
                    Kind::Domain | Kind::File | Kind::Test | Kind::Route | Kind::GeneratedRoute
                )
                && node.place.as_ref().is_some_and(|place| {
                    symbols.declarations.get(&node.name).is_some_and(|found| {
                        found
                            .iter()
                            .any(|symbol| symbol.provided.is_none() && symbol.path == place.file)
                    })
                })
        })
        .collect()
}

/// Decides how to make the move, editing nothing; a move that can't be is
/// refused.
pub fn plan(db: &dyn Db, project: Project, what: &Move) -> Result<Plan, Box<Diagnostic>> {
    let symbols = jadpo_db::symbols(db, project);
    let graph = crate::graph::graph(db, project);
    let registry = crate::graph::registry(db, project);
    let domain = what.domain().to_string();
    let to = what.to().to_string();
    if !symbols.domains.contains(&domain) {
        return Err(unknown_domain(
            &symbols.domains,
            &domain,
            |near| match what {
                Move::Rename { to, .. } => format!("jadpo domain rename {near} {to}"),
                Move::Merge { into, .. } => format!("jadpo domain merge {near} {into}"),
                Move::Split { to, names, .. } => {
                    format!("jadpo domain split {near} {to} {}", names.join(" "))
                }
            },
        ));
    }
    let mut plan = Plan {
        what: what.clone(),
        moves: Vec::new(),
        dropped: Vec::new(),
        edits: BTreeMap::new(),
        touched: BTreeSet::new(),
        lines: project.identities(db).clone(),
        line_rows: Vec::new(),
        kept: Vec::new(),
        moved: Vec::new(),
        stayed: 0,
    };
    match what {
        Move::Rename { .. } => rename(db, project, graph, registry, &domain, &to, &mut plan)?,
        Move::Merge { .. } => merge(db, project, graph, registry, &domain, &to, &mut plan)?,
        Move::Split { names, .. } => {
            split(db, project, graph, registry, &domain, &to, names, &mut plan)?;
        }
    }
    for list in plan.edits.values_mut() {
        list.sort_by_key(|edit| (edit.start, edit.end));
        list.dedup_by_key(|edit| (edit.start, edit.end));
    }
    Ok(plan)
}

/// The tables of the entities among `nodes`, which stay where they are.
fn kept(db: &dyn Db, project: Project, nodes: &[&Node]) -> Vec<String> {
    let model = crate::data::data_model(db, project);
    let mut out: Vec<String> = nodes
        .iter()
        .filter(|node| node.kind == Kind::Entity)
        .filter_map(|node| {
            model
                .entities
                .iter()
                .find(|entity| entity.born == node.key.name())
                .map(|entity| format!("table {}", entity.table))
        })
        .collect();
    out.sort();
    out
}

fn add(plan: &mut Plan, path: &str, edit: Edit) {
    plan.edits.entry(path.to_string()).or_default().push(edit);
}

fn rename(
    db: &dyn Db,
    project: Project,
    graph: &Graph,
    registry: &Registry,
    domain: &str,
    to: &str,
    plan: &mut Plan,
) -> Result<(), Box<Diagnostic>> {
    let again = format!("jadpo domain rename {domain} <new-name>");
    if to == domain {
        return Err(refused(
            format!("`{to}` is the name it has"),
            format!("A rename gives a domain a new name, and `{domain}` already has this one."),
            format!("Run `{again}` with another name"),
        ));
    }
    if jadpo_db::symbols(db, project).domains.contains(to) {
        return Err(refused(
            format!("`{to}` is already a domain"),
            format!(
                "Renaming `{domain}` to `{to}` would make two domains one, which `jadpo domain merge {domain} {to}` does on purpose."
            ),
            format!("Choose another name, then run `{again}`"),
        ));
    }
    if let Some(problem) = name_refused(db, project, to, &again) {
        return Err(problem);
    }
    let from = folder(domain);
    for (path, held) in files_of(db, project, domain) {
        let after = format!("{}{}", folder(to), &path[from.len()..]);
        plan.moves.push((path, after, held));
    }
    for site in sites(
        db,
        project,
        &domain_key(registry, domain),
        &mut plan.touched,
    ) {
        let note = (!site.in_use).then(|| format!("{to}, was {domain}"));
        add(
            plan,
            &site.path.clone(),
            Edit {
                start: site.start,
                end: site.end,
                replacement: to.to_string(),
                line: site.line,
                note,
            },
        );
    }

    // The registry: the domain's one line, and every line named after it.
    let born = domain_key(registry, domain).name().to_string();
    let had = plan.lines.remove(&format!("domain {domain}"));
    let prefix = format!("{domain}.");
    let carried: Vec<(String, String)> = plan
        .lines
        .iter()
        .filter(|(line, _)| {
            line.split_once(' ')
                .is_some_and(|(kind, name)| CARRIED.contains(&kind) && name.starts_with(&prefix))
        })
        .map(|(line, born)| (line.clone(), born.clone()))
        .collect();
    for (line, was) in carried {
        plan.lines.remove(&line);
        let (kind, name) = line.split_once(' ').unwrap_or_default();
        plan.lines
            .insert(format!("{kind} {to}.{}", &name[prefix.len()..]), was);
    }
    let needed = to != born;
    if needed {
        plan.lines.insert(format!("domain {to}"), born.clone());
    }
    let subject = format!("domain {born}");
    plan.line_rows.push(match (had.is_some(), needed) {
        (false, _) => Row {
            mark: Mark::Added,
            subject,
            note: format!("now {to}"),
        },
        (true, true) => Row {
            mark: Mark::Changed,
            subject,
            note: format!("now {to}, was {domain}"),
        },
        (true, false) => Row {
            mark: Mark::Removed,
            subject,
            note: "back to its born name".to_string(),
        },
    });
    let nodes: Vec<&Node> = graph
        .nodes()
        .iter()
        .filter(|node| node.domain.as_deref() == Some(domain))
        .collect();
    plan.kept = kept(db, project, &nodes);
    Ok(())
}

fn merge(
    db: &dyn Db,
    project: Project,
    graph: &Graph,
    registry: &Registry,
    domain: &str,
    into: &str,
    plan: &mut Plan,
) -> Result<(), Box<Diagnostic>> {
    let symbols = jadpo_db::symbols(db, project);
    if into == domain {
        return Err(refused(
            format!("`{domain}` can't merge into itself"),
            "A merge moves every file of one domain into another.".to_string(),
            format!("Run `jadpo domain merge {domain} <into>` with another domain"),
        ));
    }
    if !symbols.domains.contains(into) {
        return Err(unknown_domain(&symbols.domains, into, |near| {
            format!("jadpo domain merge {domain} {near}")
        }));
    }
    let again = format!("jadpo domain merge {domain} {into}");
    let mut pair = [domain.to_string(), into.to_string()];
    pair.sort();
    let [first, second] = &pair;

    // A private name in both: once one domain, the two would clash.
    let in_domain = |symbol: &Symbol, name: &str| {
        symbol.ownership == jadpo_db::Ownership::Domain(name.to_string())
            && symbol.provided.is_none()
            && !symbol.test
    };
    let mut clashes: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for (name, found) in &symbols.declarations {
        let ours: Vec<&Symbol> = found.iter().filter(|s| in_domain(s, domain)).collect();
        let theirs: Vec<&Symbol> = found.iter().filter(|s| in_domain(s, into)).collect();
        if !ours.is_empty() && !theirs.is_empty() {
            let mut rows: Vec<(String, String)> = theirs
                .iter()
                .chain(ours.iter())
                .map(|symbol| (symbol.name.clone(), symbol.path.clone()))
                .collect();
            rows.sort();
            clashes.push((name.clone(), rows));
        }
    }
    if let Some((name, _)) = clashes.first() {
        let names: Vec<String> = clashes.iter().map(|(name, _)| name.clone()).collect();
        let rows: Vec<(String, String)> =
            clashes.iter().flat_map(|(_, rows)| rows.clone()).collect();
        return Err(Box::new(
            Diagnostic::new(
                &rule::CLI_DOMAIN_ARGUMENTS,
                format!("`{first}` and `{second}` both declare {}", listed(&names)),
            )
            .reason(format!(
                "A private name is unique within its domain, so once `{domain}` is part of `{into}` the two would clash."
            ))
            .next(format!(
                "Rename one first, as `jadpo rename {domain}.{name} <new-name>` does, then run `{again}`"
            ))
            .in_use(rows),
        ));
    }

    // Two files at one place.
    let theirs: BTreeSet<String> = files_of(db, project, into)
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    let from = folder(domain);
    let mut taken: Vec<String> = Vec::new();
    for (path, held) in files_of(db, project, domain) {
        let rest = &path[from.len()..];
        let after = format!("{}{rest}", folder(into));
        if held == Held::Record && generated(&path) {
            plan.dropped.push(path);
            continue;
        }
        if theirs.contains(&after) {
            taken.push(rest.to_string());
        }
        plan.moves.push((path, after, held));
    }
    if !taken.is_empty() {
        return Err(refused(
            format!("`{first}` and `{second}` both have {}", listed(&taken)),
            format!(
                "A merge puts each file of `{domain}` at the same place in `{into}`, and {} taken.",
                if taken.len() == 1 {
                    "that one is"
                } else {
                    "these are"
                }
            ),
            format!("Combine them into `{into}`'s by hand first, then run `{again}`"),
        ));
    }

    // Calls between the two lose their qualifier; calls from elsewhere
    // name the domain merged into.
    let joins = |path: &str| path.starts_with(&from) || path.starts_with(&folder(into));
    for site in sites(
        db,
        project,
        &domain_key(registry, domain),
        &mut plan.touched,
    ) {
        let edit = if joins(&site.path) {
            match (&site.qualifies, site.in_use) {
                // Its `use` lines name the domain merged into, and those
                // the file no longer needs go when its imports are fixed.
                (_, true) => Some(Edit {
                    start: site.start,
                    end: site.end,
                    replacement: into.to_string(),
                    line: site.line,
                    note: None,
                }),
                (Some((dot, name, _)), false) => Some(Edit {
                    start: site.start,
                    end: *dot,
                    replacement: String::new(),
                    line: site.line,
                    note: Some(format!("{name}, was {domain}.{name}")),
                }),
                (None, false) => Some(Edit {
                    start: site.start,
                    end: site.end,
                    replacement: into.to_string(),
                    line: site.line,
                    note: Some(format!("{into}, was {domain}")),
                }),
            }
        } else {
            Some(Edit {
                start: site.start,
                end: site.end,
                replacement: into.to_string(),
                line: site.line,
                note: (!site.in_use).then(|| format!("{into}, was {domain}")),
            })
        };
        if let Some(edit) = edit {
            add(plan, &site.path.clone(), edit);
        }
    }
    for site in sites(db, project, &domain_key(registry, into), &mut plan.touched) {
        if !site.path.starts_with(&from) || site.in_use {
            continue;
        }
        if let Some((dot, name, _)) = &site.qualifies {
            add(
                plan,
                &site.path.clone(),
                Edit {
                    start: site.start,
                    end: *dot,
                    replacement: String::new(),
                    line: site.line,
                    note: Some(format!("{name}, was {into}.{name}")),
                },
            );
        }
    }

    // The registry: each key that carries the domain keeps its born name,
    // and the domain's own line goes with the domain.
    let nodes: Vec<&Node> = graph
        .nodes()
        .iter()
        .filter(|node| node.domain.as_deref() == Some(domain))
        .collect();
    let carrying: Vec<&Node> = nodes
        .iter()
        .copied()
        .filter(|node| carries_domain(graph, registry, node, domain))
        .collect();
    let mut rows = Vec::new();
    carried_lines(&mut plan.lines, &carrying, domain, into, &mut rows);
    if let Some(born) = plan.lines.remove(&format!("domain {domain}")) {
        rows.push(Row {
            mark: Mark::Removed,
            subject: format!("domain {born}"),
            note: format!("merged into {into}"),
        });
    }
    plan.line_rows = rows;
    plan.kept = kept(db, project, &nodes);
    Ok(())
}

/// A reference that crosses a split: `add_todo` writes `Todo`.
fn crossing(graph: &Graph, from: &Key, kind: jadpo_graph::EdgeKind, to: &Key) -> String {
    let named = |key: &Key| -> String {
        let Some(node) = graph.get(key) else {
            return key.written().to_string();
        };
        let owner = node
            .parent
            .as_ref()
            .and_then(|parent| graph.get(parent))
            .filter(|_| matches!(node.kind, Kind::Field | Kind::Variant | Kind::Rule));
        match (node.kind, owner) {
            (Kind::Test, _) => format!("the test \"{}\"", node.name),
            (_, Some(owner)) => format!("`{}`", owner.name),
            _ => format!("`{}`", node.name),
        }
    };
    let verb = match kind.word() {
        "subscribes" => "subscribes to",
        word => word,
    };
    format!("{} {verb} {}", named(from), named(to))
}

#[allow(clippy::too_many_arguments)]
fn split(
    db: &dyn Db,
    project: Project,
    graph: &Graph,
    registry: &Registry,
    domain: &str,
    to: &str,
    names: &[String],
    plan: &mut Plan,
) -> Result<(), Box<Diagnostic>> {
    let symbols = jadpo_db::symbols(db, project);
    let named = names.join(" ");
    if names.is_empty() {
        return Err(refused(
            "`jadpo domain split` needs what to move".to_string(),
            "A split moves the declarations you name, with their tests, to a new domain, and leaves the rest where it is.".to_string(),
            format!("Run `jadpo domain split {domain} {to} <name>...`, naming the declarations that move"),
        ));
    }
    let again = format!("jadpo domain split {domain} <new-domain> {named}");
    if symbols.domains.contains(to) {
        return Err(refused(
            format!("`{to}` is already a domain"),
            "A split makes a new domain; `jadpo domain merge` moves one domain into another that exists.".to_string(),
            format!("Choose another name, then run `{again}`"),
        ));
    }
    if let Some(problem) = name_refused(db, project, to, &again) {
        return Err(problem);
    }

    // The files that move: each named declaration's, the test beside it,
    // and its templates and prompts.
    let all = declarations(db, project, graph, domain);
    let mut chosen: BTreeSet<String> = BTreeSet::new();
    for name in names {
        let found = all.iter().find(|node| node.name == *name);
        let Some(node) = found else {
            let elsewhere = graph
                .nodes()
                .iter()
                .find(|node| node.name == *name && node.parent.is_none() && node.place.is_some());
            return Err(match elsewhere {
                Some(other) => refused(
                    format!("`{name}` isn't in `{domain}`"),
                    format!(
                        "A split moves declarations of the domain it splits, and `{name}` is declared in `{}`.",
                        other.place.as_ref().map_or("", |place| place.file.as_str())
                    ),
                    format!("Name declarations of `{domain}`; `jadpo inspect` lists them"),
                ),
                None => {
                    let near = nearest(name, all.iter().map(|node| node.name.as_str()));
                    refused(
                        format!("`{name}` isn't a declaration of `{domain}`"),
                        match near {
                            Some(near) => format!(
                                "`{domain}` declares nothing called `{name}`. Did you mean `{near}`?"
                            ),
                            None => format!("`{domain}` declares nothing called `{name}`."),
                        },
                        format!("Name declarations of `{domain}`; `jadpo inspect` lists them"),
                    )
                }
            });
        };
        if let Some(place) = &node.place {
            chosen.insert(place.file.clone());
        }
    }
    let files = files_of(db, project, domain);
    let declared: BTreeSet<String> = all
        .iter()
        .filter_map(|node| node.place.as_ref().map(|place| place.file.clone()))
        .collect();
    if declared.iter().all(|file| chosen.contains(file)) {
        return Err(refused(
            format!("Everything in `{domain}` would move"),
            "A split leaves the rest of the domain where it is; to give all of it another name, rename the domain.".to_string(),
            format!("Run `jadpo domain rename {domain} {to}`"),
        ));
    }

    // A seam: no reference across it, either way.
    let file_of = |key: &Key| -> Option<String> {
        graph
            .get(key)
            .and_then(|node| node.place.as_ref())
            .map(|place| place.file.clone())
    };
    let subject_of = |path: &str| -> String {
        // A test file goes with the file it sits beside.
        path.strip_suffix(".test.jadpo")
            .map_or(path.to_string(), |stem| format!("{stem}.jadpo"))
    };
    let mut links: Vec<(String, String, String)> = Vec::new();
    for file in project.files(db) {
        let path = file.path(db);
        if !path.starts_with(&folder(domain)) {
            continue;
        }
        let from = subject_of(path);
        for (source, kind, target) in &crate::graph::file_part(db, project, *file).edges {
            let Some(target_file) = file_of(target) else {
                continue;
            };
            if !target_file.starts_with(&folder(domain)) {
                continue;
            }
            let target_file = subject_of(&target_file);
            if target_file != from {
                links.push((
                    from.clone(),
                    target_file,
                    crossing(graph, source, *kind, target),
                ));
            }
        }
    }
    let across: Vec<String> = {
        let mut out: Vec<String> = links
            .iter()
            .filter(|(from, target, _)| chosen.contains(from) != chosen.contains(target))
            .map(|(_, _, said)| said.clone())
            .collect();
        out.sort();
        out.dedup();
        out
    };
    if !across.is_empty() {
        // What the named reach together, either way.
        let mut part = chosen.clone();
        loop {
            let more: Vec<String> = links
                .iter()
                .filter_map(|(from, target, _)| {
                    match (part.contains(from), part.contains(target)) {
                        (true, false) => Some(target.clone()),
                        (false, true) => Some(from.clone()),
                        _ => None,
                    }
                })
                .collect();
            if more.is_empty() {
                break;
            }
            part.extend(more);
        }
        const SHOWN: usize = 3;
        let mut said: Vec<String> = across.iter().take(SHOWN).cloned().collect();
        if across.len() > SHOWN {
            said.push(format!("{} more", across.len() - SHOWN));
        }
        let next = if declared.iter().all(|file| part.contains(file)) {
            format!(
                "Keep `{domain}` as one domain: nothing in it moves without what it reaches, and a domain whose parts depend on each other stays large"
            )
        } else {
            let mut moving: Vec<String> = all
                .iter()
                .filter(|node| {
                    node.place
                        .as_ref()
                        .is_some_and(|place| part.contains(&place.file))
                })
                .map(|node| node.name.clone())
                .collect();
            moving.sort();
            moving.dedup();
            format!(
                "Move what they reach too: `jadpo domain split {domain} {to} {}`",
                moving.join(" ")
            )
        };
        return Err(refused(
            format!("What moves and what stays in `{domain}` reference each other"),
            format!(
                "A domain splits only where nothing references across it: {}.",
                crate::and_list(&said)
            ),
            next,
        ));
    }

    // Moved: the chosen files, the tests beside them, and what is named
    // after them in their folders (an email's templates, a prompt's text
    // and recordings).
    let from = folder(domain);
    for (path, held) in &files {
        let rest = &path[from.len()..];
        let goes = chosen.iter().any(|chosen| {
            let Some(stem) = chosen.strip_suffix(".jadpo") else {
                return false;
            };
            let (dir, name) = stem.rsplit_once('/').unwrap_or(("", stem));
            path == chosen
                || path.starts_with(&format!("{stem}."))
                || path.starts_with(&format!("{dir}/recordings/{name}/"))
        });
        if goes && !(*held == Held::Record && generated(path)) {
            plan.moves
                .push((path.clone(), format!("{}{rest}", folder(to)), *held));
        }
    }

    // Calls into what moved name the new domain; a setting that lists the
    // domain lists the new one too.
    let moved_keys: BTreeSet<&Key> = all
        .iter()
        .filter(|node| {
            node.place
                .as_ref()
                .is_some_and(|place| chosen.contains(&place.file))
        })
        .map(|node| &node.key)
        .collect();
    for site in sites(
        db,
        project,
        &domain_key(registry, domain),
        &mut plan.touched,
    ) {
        if site.in_use {
            continue;
        }
        let edit = match &site.qualifies {
            Some((_, _, Some(key))) if moved_keys.contains(key) => Edit {
                start: site.start,
                end: site.end,
                replacement: to.to_string(),
                line: site.line,
                note: Some(format!("{to}, was {domain}")),
            },
            Some(_) => continue,
            None => Edit {
                start: site.start,
                end: site.end,
                replacement: format!("{domain}, {to}"),
                line: site.line,
                note: Some(format!("{domain}, {to}, was {domain}")),
            },
        };
        add(plan, &site.path.clone(), edit);
    }

    let moving: Vec<&Node> = all
        .iter()
        .copied()
        .filter(|node| moved_keys.contains(&node.key))
        .collect();
    // What carries the domain in its key, the failure the compiler
    // provides for a moved entity among it.
    let carrying: Vec<&Node> = graph
        .nodes()
        .iter()
        .filter(|node| {
            node.place
                .as_ref()
                .is_some_and(|place| chosen.contains(&place.file))
                && carries_domain(graph, registry, node, domain)
        })
        .collect();
    let mut rows = Vec::new();
    carried_lines(&mut plan.lines, &carrying, domain, to, &mut rows);
    plan.line_rows = rows;
    plan.kept = kept(db, project, &moving);
    let mut moved: Vec<String> = moving.iter().map(|node| node.name.clone()).collect();
    moved.sort();
    plan.moved = moved;
    plan.stayed = all.len() - moving.len();
    Ok(())
}

/// Makes the plan's edits and moves in `loaded`, writes the registry and
/// the records the move puts out of date, then fixes imports, formats and
/// checks; undone, with the errors it would add, when the result has an
/// error the project didn't have.
pub fn apply(loaded: &mut Loaded, plan: &Plan) -> Result<Moved, Stopped> {
    let project = loaded.project;
    let before_errors = headlines(&crate::check(&loaded.db, project));
    let before_records = record_states(&loaded.db, project);
    let text_of =
        |loaded: &Loaded, path: &str| loaded.file(path).map(|file| file.text(&loaded.db).clone());

    // What each touched file held before.
    let mut paths: BTreeSet<String> = plan.edits.keys().cloned().collect();
    paths.extend(plan.touched.iter().cloned());
    for (from, to, held) in &plan.moves {
        if *held == Held::Source {
            paths.insert(from.clone());
            paths.insert(to.clone());
        }
    }
    let before: BTreeMap<String, Option<String>> = paths
        .iter()
        .map(|path| (path.clone(), text_of(loaded, path)))
        .collect();
    let mut records_before: BTreeMap<String, Option<String>> = BTreeMap::new();
    records_before.insert(
        jadpo_db::IDENTITIES.to_string(),
        loaded.record(jadpo_db::IDENTITIES),
    );
    for (from, to, held) in &plan.moves {
        if *held == Held::Record {
            records_before.insert(from.clone(), loaded.record(from));
            records_before.insert(to.clone(), loaded.record(to));
        }
    }
    for path in &plan.dropped {
        records_before.insert(path.clone(), loaded.record(path));
    }
    let others_before: Vec<String> = loaded.project.other_files(&loaded.db).clone();

    // 1. Every place the domain is written.
    for (path, edits) in &plan.edits {
        let text = text_of(loaded, path).unwrap_or_default();
        let replacements = edits
            .iter()
            .map(|edit| (edit.start, edit.end, edit.replacement.clone()))
            .collect();
        loaded.set_text(path, crate::apply_edits(&text, replacements));
    }

    // 2. The files that move.
    for (from, to, held) in &plan.moves {
        match held {
            Held::Source => {
                if let Some(text) = text_of(loaded, from) {
                    loaded.remove(from);
                    loaded.set_text(to, text);
                }
            }
            Held::Record => {
                let text = loaded.record(from);
                loaded.set_record(from, None);
                loaded.set_record(to, text);
            }
            Held::Other => {
                loaded.set_other(from, false);
                loaded.set_other(to, true);
            }
        }
    }
    for path in &plan.dropped {
        loaded.set_record(path, None);
    }
    let present: Vec<String> = paths
        .iter()
        .filter(|path| loaded.file(path).is_some())
        .cloned()
        .collect();
    for path in &present {
        crate::fix_imports(loaded, path);
    }

    // 3. Formatted as the files were.
    let moved_from: BTreeMap<&str, &str> = plan
        .moves
        .iter()
        .map(|(from, to, _)| (to.as_str(), from.as_str()))
        .collect();
    format_as_before(loaded, &present, &before, &moved_from);

    // 4. Merged, what was offered to the other domain alone is offered to
    // no one, so it is no longer `public`.
    let mut before = before;
    let mut paths = paths;
    let mut unpublic: BTreeMap<String, Vec<Row>> = BTreeMap::new();
    if let Move::Merge { into, .. } = &plan.what {
        let inside = folder(into);
        for _ in 0..4 {
            let problems = crate::check(&loaded.db, project);
            let mut edits: BTreeMap<String, Vec<(u32, u32, String)>> = BTreeMap::new();
            for problem in added_errors(before_errors.clone(), &problems) {
                if problem.rule.id != rule::VISIBILITY_PUBLIC_UNUSED.id {
                    continue;
                }
                let Some(next) = &problem.next else { continue };
                for edit in &next.edits {
                    if edit.span.path.starts_with(&inside) {
                        edits.entry(edit.span.path.clone()).or_default().push((
                            edit.span.start,
                            edit.span.end,
                            edit.replacement.clone(),
                        ));
                    }
                }
            }
            if edits.is_empty() {
                break;
            }
            for (path, edits) in edits {
                let text = text_of(loaded, &path).unwrap_or_default();
                before
                    .entry(path.clone())
                    .or_insert_with(|| Some(text.clone()));
                paths.insert(path.clone());
                for (start, _, _) in &edits {
                    let line = text[..*start as usize].matches('\n').count() as u32 + 1;
                    unpublic.entry(path.clone()).or_default().push(Row {
                        mark: Mark::Changed,
                        subject: format!("line {line}"),
                        note: format!("no longer public, as only {into} uses it"),
                    });
                }
                loaded.set_text(&path, crate::apply_edits(&text, edits));
            }
        }
    }

    // 5. The registry, then each record the move put out of date.
    let registry_changed = plan.lines != *project.identities(&loaded.db);
    if registry_changed {
        loaded.set_record(
            jadpo_db::IDENTITIES,
            Some(jadpo_db::write_identities(&plan.lines)),
        );
    }
    let rewritten = rewrite_records(loaded, &before_records, &mut records_before);

    // 6. Checked: a move never adds an error.
    let problems = crate::check(&loaded.db, project);
    let added = added_errors(before_errors, &problems);
    let after: BTreeMap<String, Option<String>> = paths
        .iter()
        .map(|path| (path.clone(), text_of(loaded, path)))
        .collect();
    let records_after: BTreeMap<String, Option<String>> = records_before
        .keys()
        .map(|path| (path.clone(), loaded.record(path)))
        .collect();
    if !added.is_empty() {
        let sources = jadpo_db::source_texts(&loaded.db, project);
        crate::setting_move::restore_texts(loaded, &before);
        for (path, text) in &records_before {
            loaded.set_record(path, text.clone());
        }
        for (from, to, held) in &plan.moves {
            if *held == Held::Other {
                loaded.set_other(to, others_before.contains(to));
                loaded.set_other(from, others_before.contains(from));
            }
        }
        return Err(Stopped {
            problems: added,
            sources,
        });
    }

    // What to write: every text that changed.
    let mut files: Vec<FileChange> = Vec::new();
    for path in &paths {
        let was = before.get(path).cloned().flatten();
        let now = after.get(path).cloned().flatten();
        if was != now {
            files.push(FileChange {
                path: path.clone(),
                before: was,
                after: now,
                rows: Vec::new(),
            });
        }
    }
    let mut records = false;
    for (path, was) in &records_before {
        let now = records_after.get(path).cloned().flatten();
        if *was != now {
            if path != jadpo_db::IDENTITIES && rewritten.contains(path) {
                records = true;
            }
            files.push(FileChange {
                path: path.clone(),
                before: was.clone(),
                after: now,
                rows: Vec::new(),
            });
        }
    }
    let others: Vec<(String, String)> = plan
        .moves
        .iter()
        .filter(|(_, _, held)| *held == Held::Other)
        .map(|(from, to, _)| (from.clone(), to.clone()))
        .collect();

    Ok(Moved {
        groups: groups(
            plan,
            &unpublic,
            &rewritten,
            &records_before,
            &records_after,
            registry_changed,
        ),
        files,
        others,
        errors_left: problems.len(),
        records,
    })
}

/// The change list: the folders first, then each file with an edit shown,
/// by its path after the move, then the records written again and the
/// registry.
fn groups(
    plan: &Plan,
    unpublic: &BTreeMap<String, Vec<Row>>,
    rewritten: &[String],
    records_before: &BTreeMap<String, Option<String>>,
    records_after: &BTreeMap<String, Option<String>>,
    registry_changed: bool,
) -> Vec<(String, Vec<Row>)> {
    let domain = plan.what.domain();
    let to = plan.what.to();
    let count = plan.moves.len() + plan.dropped.len();
    let files = |n: usize| {
        if n == 1 {
            "1 file".to_string()
        } else {
            format!("{n} files")
        }
    };
    let mut out: Vec<(String, Vec<Row>)> = Vec::new();
    let path_of = |domain: &str| format!("domains/{domain}");
    match &plan.what {
        Move::Rename { .. } => {
            out.push((
                path_of(domain),
                vec![Row {
                    mark: Mark::Removed,
                    subject: domain.to_string(),
                    note: format!("folder of {}, renamed {to}", files(count)),
                }],
            ));
            out.push((
                path_of(to),
                vec![Row {
                    mark: Mark::Added,
                    subject: to.to_string(),
                    note: format!("folder, was {domain}"),
                }],
            ));
        }
        Move::Merge { .. } => out.push((
            path_of(domain),
            vec![Row {
                mark: Mark::Removed,
                subject: domain.to_string(),
                note: format!("folder of {}, merged into {to}", files(count)),
            }],
        )),
        Move::Split { .. } => {
            let from = folder(domain);
            out.push((
                path_of(domain),
                plan.moves
                    .iter()
                    .map(|(path, _, _)| Row {
                        mark: Mark::Removed,
                        subject: path[from.len()..].to_string(),
                        note: format!("moved to {to}"),
                    })
                    .collect(),
            ));
            out.push((
                path_of(to),
                vec![Row {
                    mark: Mark::Added,
                    subject: to.to_string(),
                    note: format!("new domain, {} from {domain}", files(plan.moves.len())),
                }],
            ));
        }
    }
    let after_move = |path: &str| -> String {
        plan.moves
            .iter()
            .find(|(from, _, _)| from == path)
            .map_or(path.to_string(), |(_, to, _)| to.clone())
    };
    let mut edited: Vec<(String, Vec<Row>)> = plan
        .edits
        .iter()
        .filter_map(|(path, edits)| {
            let mut seen = BTreeSet::new();
            let rows: Vec<Row> = edits
                .iter()
                .filter_map(|edit| {
                    let note = edit.note.clone()?;
                    seen.insert(edit.line).then(|| Row {
                        mark: Mark::Changed,
                        subject: format!("line {}", edit.line),
                        note,
                    })
                })
                .collect();
            (!rows.is_empty()).then(|| (after_move(path), rows))
        })
        .collect();
    for (path, rows) in unpublic {
        match edited.iter_mut().find(|(shown, _)| shown == path) {
            Some((_, shown)) => {
                shown.extend(rows.iter().cloned());
                shown.sort_by_key(|row| {
                    row.subject
                        .strip_prefix("line ")
                        .and_then(|line| line.parse::<u32>().ok())
                });
            }
            None => edited.push((path.clone(), rows.clone())),
        }
    }
    edited.sort_by(|a, b| a.0.cmp(&b.0));
    out.extend(edited);
    let mut shown: BTreeSet<&String> = BTreeSet::new();
    for path in rewritten {
        if path == jadpo_db::IDENTITIES || !shown.insert(path) {
            continue;
        }
        if records_before.get(path) == records_after.get(path) {
            continue;
        }
        out.push((
            path.clone(),
            vec![Row {
                mark: Mark::Changed,
                subject: "record".to_string(),
                note: "brought up to date".to_string(),
            }],
        ));
    }
    if registry_changed && !plan.line_rows.is_empty() {
        out.push((jadpo_db::IDENTITIES.to_string(), plan.line_rows.clone()));
    }
    out
}
