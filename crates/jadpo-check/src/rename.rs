//! Renaming a declaration or a field (area 14, "Commands for looking into
//! and changing code"; area 18, "Stable identity"; area 5, "Migrations"):
//! `jadpo rename` runs this code, and so does the editor's rename
//! ([`at`] finds what the name under the cursor is), so both rename the
//! same places and refuse the same names.
//!
//! - **What it renames.** An entity, a type of any kind, a function, a
//!   constant, an action, a query, a fixture, a seed, a schedule, a
//!   subscriber, a library and a library's function or constant; and a
//!   field or variant of an entity or a type. A domain moves its folder,
//!   which `jadpo domain` does; a route goes by its method and path, and a
//!   test by its sentence, both written where they are.
//! - **Where.** Every name [`node_references`] finds referring to it, the
//!   resolution find references and go to definition use, so a name of
//!   the same spelling that means something else (a parameter, another
//!   domain's private type) is left alone. A field's new name reaches the
//!   fields copied from it (`pick:`), and what reads those. Migration
//!   folders are history, and an installed package is its own, so neither
//!   is edited. A declaration in the file named after it moves to the new
//!   name's file, with the test file beside it.
//! - **Keeping data.** The identity registry gets the line that maps the
//!   name now to the name it was born with, so a stored field keeps its
//!   column and an entity its table, and everything stored under a key (a
//!   subscriber's deliveries, a schedule's state) still names it. A second
//!   rename changes the line; a rename back to the born name removes it; a
//!   type or entity renamed carries its members' lines with it.
//! - **Records.** Each view record the rename puts out of date (the API,
//!   event, access and network records) is written again, as `jadpo
//!   records update` writes it, so the rename leaves `jadpo check` as it
//!   found it. Whether the change needs a person's acceptance is judged by
//!   `jadpo check`, against the before side, as for any change.
//! - **Refused** ([`plan`]): a name that isn't one for its kind, the name it
//!   has, a built-in type's or a standard namespace's name, and a name in
//!   use where the new one has to be unique (its owner's members for a
//!   field or variant, its library for a library's function, its domain for
//!   a private function or type, the project for everything else), with
//!   the names in use that share its words, as
//!   `semantic.duplicate_declaration` lists them.
//! - **Stopped** ([`apply`]): a result with an error the project didn't
//!   have, such as a type that copies the field already having one of the
//!   new name, is undone and refused with those errors. Errors the
//!   project already had stay, and are counted.
//!
//! As in [`crate::fixes`], [`plan`] reads and decides, editing nothing, and
//! [`apply`] edits the loaded project's texts and records, fixes the
//! imports of each file it touched, formats each that was formatted and
//! checks the result. The command writes what [`apply`] gives back.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, Loaded, Project, Symbol};
use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::{Detail, Graph, Key, Kind, Node, Registry};
use jadpo_syntax::SyntaxKind::USE_DECL;
use jadpo_syntax::ast::AstNode;

use crate::case::{is_lower_snake, is_upper_camel, round_trips, to_snake};
use crate::graph::node_references;
use crate::setting_move::{FileChange, Mark, Row, Stopped};
use crate::std_lib::StdNames;

/// Whether `jadpo rename` renames a node of `kind`.
pub fn renames(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Entity
            | Kind::Type
            | Kind::Function
            | Kind::Constant
            | Kind::Action
            | Kind::Query
            | Kind::Fixture
            | Kind::Seed
            | Kind::Schedule
            | Kind::Subscriber
            | Kind::Library
            | Kind::Field
            | Kind::Variant
    )
}

/// What `jadpo rename` renames, in the reader's words.
const RENAMES: &str = "`jadpo rename` renames entities, types, functions, constants, actions, queries, fixtures, seeds, schedules, subscribers and libraries, and their fields and variants.";

/// `cli.rename_arguments` for `name`, a node of `kind` that `jadpo
/// rename` doesn't rename, saying what changes it instead; `None` for a
/// kind it renames.
pub fn not_renamed(name: &str, kind: Kind) -> Option<Diagnostic> {
    if renames(kind) {
        return None;
    }
    let refusal = |message: String, reason: &str, next: &str| {
        Diagnostic::new(&rule::CLI_RENAME_ARGUMENTS, message)
            .reason(reason)
            .next(next)
    };
    Some(match kind {
        Kind::Route | Kind::GeneratedRoute => refusal(
            format!("`{name}` is a route"),
            "A route goes by its method and path, written in its `routes` block.",
            "Change its method or path where its `routes` block writes them",
        ),
        Kind::Test => refusal(
            format!("`{name}` is a test"),
            "A test goes by the file it sits beside and its sentence, written where the test is.",
            "Change its sentence where the test is written",
        ),
        kind => refusal(
            format!(
                "`{name}` is {} {}",
                a_or_an(kind.word()),
                kind.word().replace('_', " ")
            ),
            RENAMES,
            "Change it where it is written",
        ),
    })
}

/// The project's syntax errors, which stop a rename before it starts:
/// editing a file that doesn't parse could change the wrong text, as in
/// every command that writes source.
pub fn unparsed(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    crate::check(db, project)
        .into_iter()
        .filter(|problem| problem.rule.id.starts_with("syntax."))
        .collect()
}

/// What a name in the editor is to a rename: the node it refers to, or
/// why `jadpo rename` doesn't rename it; with the name's own byte range.
pub type Target = (Result<Key, Box<Diagnostic>>, (u32, u32));

/// What the editor's rename at byte `offset` of `file` renames: the node
/// the name there refers to, as go to definition resolves it, or why
/// `jadpo rename` doesn't rename it; with the name's own byte range.
/// `None` when no name is there.
pub fn at(db: &dyn Db, project: Project, file: jadpo_db::File, offset: u32) -> Option<Target> {
    let (found, range) = crate::graph::lookup(db, project, file, offset)?;
    let target = match found {
        crate::graph::Found::Node(key) => {
            let node = crate::graph::graph(db, project).get(&key)?;
            match not_renamed(&node.name, node.kind) {
                Some(refusal) => Err(Box::new(refusal)),
                None => Ok(key),
            }
        }
        crate::graph::Found::Local { name, what, .. } => Err(Box::new(
            Diagnostic::new(
                &rule::CLI_RENAME_ARGUMENTS,
                format!("`{name}` is {} {what}", a_or_an(what)),
            )
            .reason(RENAMES)
            .next("Change it where it is written"),
        )),
    };
    Some((target, range))
}

/// The kinds whose names are written in UpperCamelCase.
fn upper(kind: Kind) -> bool {
    matches!(kind, Kind::Entity | Kind::Type)
}

/// The kinds whose name is their owner's and their own (`Todo.priority`).
fn member(kind: Kind) -> bool {
    matches!(kind, Kind::Field | Kind::Variant | Kind::Rule)
}

/// The registry's kinds that follow an entity or type: its members.
const MEMBERS: &[&str] = &["field", "variant", "rule"];
/// The registry's kinds that follow a library: its functions and constants.
const LIBRARY_MEMBERS: &[&str] = &["function", "constant"];

/// One text replacement, with its line in the text before.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Edit {
    start: u32,
    end: u32,
    line: u32,
    /// Whether the change list shows it; `use` lines are left to the diff.
    shown: bool,
}

/// What [`plan`] decided.
#[derive(Clone, Debug)]
pub struct Plan {
    pub key: Key,
    pub kind: Kind,
    /// The kind in the reader's words: `field`, `entity`, `failure`.
    pub word: &'static str,
    /// Its name now, as code writes it: `Todo.priority`.
    pub name: String,
    /// Its name after: `Todo.importance`.
    pub to: String,
    /// Its own part of each: `priority` and `importance`.
    own: String,
    new_own: String,
    /// Each file's replacements, in path order.
    edits: BTreeMap<String, Vec<Edit>>,
    /// The files that move: the declaration's own, and the test beside it.
    moves: Vec<(String, String)>,
    /// The registry's lines after, keyed `kind name-now` to the born name,
    /// and the row that says what changed in it.
    lines: BTreeMap<String, String>,
    line: Row,
    /// What stored data the rename keeps: `table todo`, `column
    /// todo.priority`.
    pub kept: Option<String>,
}

impl Plan {
    /// The files that move, each path before and after: the declaration's
    /// own, then the test beside it.
    pub fn moves(&self) -> &[(String, String)] {
        &self.moves
    }
}

/// What [`apply`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Renamed {
    /// Every file it changed: the declaration's first, then the rest in
    /// path order, then the records, the registry last.
    pub files: Vec<FileChange>,
    /// The errors the project still has, every one of which it had
    /// before.
    pub errors_left: usize,
    /// Whether a record other than the registry was written again: one a
    /// person may need to accept the change to.
    pub records: bool,
}

/// `cli.rename_arguments`, with its reason and next step.
fn refused(message: String, reason: String, next: String) -> Box<Diagnostic> {
    Box::new(
        Diagnostic::new(&rule::CLI_RENAME_ARGUMENTS, message)
            .reason(reason)
            .next(next),
    )
}

/// A name as typed at a shell: quoted when it holds a space.
fn shell(name: &str) -> String {
    if name.contains(' ') {
        format!("\"{name}\"")
    } else {
        name.to_string()
    }
}

fn a_or_an(word: &str) -> &'static str {
    if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

/// The node's kind in the reader's words.
fn word_of(db: &dyn Db, project: Project, node: &Node) -> &'static str {
    match node.kind {
        Kind::Field => "field",
        Kind::Variant => "variant",
        Kind::Library => "library",
        _ => symbol_of(db, project, node).map_or(node.kind.word(), |symbol| symbol.kind.word()),
    }
}

/// The declaration a node of a declared kind is: the symbol of its name in
/// its file.
fn symbol_of(db: &dyn Db, project: Project, node: &Node) -> Option<Symbol> {
    let place = node.place.as_ref()?;
    let own = node.name.rsplit('.').next().unwrap_or(&node.name);
    jadpo_db::symbols(db, project)
        .declarations
        .get(own)?
        .iter()
        .find(|symbol| symbol.path == place.file && symbol.provided.is_none())
        .cloned()
}

/// The name a node goes by in the registry's lines now: its name, with
/// the domain for a function or type declared in one, under its owner's
/// for a member or a library's function.
fn identity_now(graph: &Graph, registry: &Registry, node: &Node) -> String {
    let parent = node.parent.as_ref().and_then(|key| graph.get(key));
    match parent {
        Some(owner) if member(node.kind) || owner.kind == Kind::Library => {
            let path = node
                .name
                .strip_prefix(&format!("{}.", owner.name))
                .unwrap_or_else(|| node.name.rsplit('.').next().unwrap_or(&node.name));
            format!("{}.{path}", identity_now(graph, registry, owner))
        }
        _ => {
            let mut candidates = vec![node.name.clone()];
            if let Some(domain) = &node.domain {
                candidates.push(format!("{domain}.{}", node.name));
            }
            candidates
                .iter()
                .find(|now| registry.key(node.kind, now, None) == node.key)
                .cloned()
                .unwrap_or_else(|| node.name.clone())
        }
    }
}

/// `name` with its last part `own`.
fn with_own(name: &str, own: &str) -> String {
    match name.rsplit_once('.') {
        Some((owner, _)) => format!("{owner}.{own}"),
        None => own.to_string(),
    }
}

/// Decides how to rename the node `key` to `new`, editing nothing; a new
/// name that can't be is refused.
pub fn plan(db: &dyn Db, project: Project, key: &Key, new: &str) -> Result<Plan, Box<Diagnostic>> {
    let graph = crate::graph::graph(db, project);
    let Some(node) = graph.get(key) else {
        return Err(refused(
            format!("`{}` isn't a declaration", key.written()),
            "Nothing in the project has that identity now.".to_string(),
            "Run `jadpo inspect` to list the project's declarations".to_string(),
        ));
    };
    let name = node.name.clone();
    let kind = node.kind;
    let word = word_of(db, project, node);
    let parent = node.parent.as_ref().and_then(|key| graph.get(key));
    let owner = parent.filter(|owner| member(kind) || owner.kind == Kind::Library);
    let own = match owner {
        Some(owner) => name
            .strip_prefix(&format!("{}.", owner.name))
            .unwrap_or(&name)
            .to_string(),
        None => name.rsplit('.').next().unwrap_or(&name).to_string(),
    };
    let again = format!("jadpo rename {} <new-name>", shell(&name));

    // The new name: its own, or after its owner's.
    let new_own = match (owner, new.rsplit_once('.')) {
        (_, None) => new.to_string(),
        (Some(owner), Some((prefix, rest))) if prefix == owner.name => rest.to_string(),
        (Some(owner), Some((_, rest))) => {
            let (what, keeps) = if owner.kind == Kind::Library {
                ("library", "in its library")
            } else {
                ("", "where it is")
            };
            let of = if what.is_empty() {
                format!("`{}`", owner.name)
            } else {
                format!("the {what} `{}`", owner.name)
            };
            return Err(refused(
                format!("`{new}` isn't {} {word} of {of}", a_or_an(word)),
                format!(
                    "A rename keeps {} {word} {keeps}, so its new name is its own (`{rest}`) or after `{}` (`{}.{rest}`).",
                    a_or_an(word),
                    owner.name,
                    owner.name
                ),
                format!("Run `jadpo rename {} {rest}`", shell(&name)),
            ));
        }
        (None, Some(_)) => {
            return Err(refused(
                format!("`{new}` isn't a name for {} {word}", a_or_an(word)),
                format!("A rename gives `{name}` a new name of its own, and a name holds no `.`."),
                format!("Run `{again}` with a name of one part"),
            ));
        }
    };

    // A name for its kind.
    if upper(kind) {
        if !is_upper_camel(&new_own) || !round_trips(&new_own) {
            return Err(refused(
                format!("`{new_own}` isn't a name for {} {word}", a_or_an(word)),
                format!(
                    "{} {word}'s name is written in UpperCamelCase and reads back from its file name unchanged, as `RoomReference` does from `room_reference.jadpo`.",
                    capitalised(a_or_an(word))
                ),
                format!("Run `{again}` with an UpperCamelCase name"),
            ));
        }
    } else if !is_lower_snake(&new_own) {
        let example = to_snake(&new_own);
        let example = if is_lower_snake(&example) {
            format!(", as `{example}` is")
        } else {
            String::new()
        };
        return Err(refused(
            format!("`{new_own}` isn't a name for {} {word}", a_or_an(word)),
            format!(
                "{} {word}'s name is written in lower_snake_case{example}.",
                capitalised(a_or_an(word))
            ),
            format!("Run `{again}` with a lower_snake_case name"),
        ));
    }
    if new_own == own {
        return Err(refused(
            format!("`{new_own}` is the name it has"),
            format!(
                "A rename gives a declaration or a field a new name, and `{name}` already has this one."
            ),
            format!("Run `{again}` with another name"),
        ));
    }
    let names = StdNames::get();
    if upper(kind) && (jadpo_std::standard_type(&new_own).is_some() || names.is_type(&new_own)) {
        return Err(refused(
            format!("`{new_own}` is a built-in type"),
            "Built-in and standard types exist without being declared, and their names can't be reused.".to_string(),
            format!("Run `{again}` with another name"),
        ));
    }
    if !upper(kind) && owner.is_none() && names.reserved(&new_own) {
        return Err(refused(
            format!("`{new_own}` is a standard-library namespace"),
            "Standard-library namespaces can't name a declaration.".to_string(),
            format!("Run `{again}` with another name"),
        ));
    }

    // Not a name in use.
    let to = with_own(&name, &new_own);
    if let Some(owner) = owner
        && let Some(problem) = member_in_use(graph, node, owner, &name, &to, &new_own)
    {
        return Err(problem);
    }
    if !member(kind)
        && let Some(problem) = declaration_in_use(db, project, node, &name, &new_own, &again)
    {
        return Err(problem);
    }

    // Every place it is written, and every field copied from it.
    let mut keys = vec![key.clone()];
    if kind == Kind::Field {
        keys.extend(copies(graph, key));
    }
    let edits = references(db, project, &keys);

    // The file named after it, and the test beside it.
    let mut moves = Vec::new();
    if owner.is_none()
        && let Some(place) = &node.place
        && project.layout(db) == jadpo_db::Layout::Project
    {
        let location = jadpo_db::locate(&place.file, project.layout(db));
        if !location.test && location.stem == to_snake(&own) {
            let folder = place
                .file
                .rsplit_once('/')
                .map_or(String::new(), |(folder, _)| format!("{folder}/"));
            let stem = to_snake(&new_own);
            let mut pairs = vec![(place.file.clone(), format!("{folder}{stem}.jadpo"))];
            let test = format!("{folder}{}.test.jadpo", location.stem);
            if file_at(db, project, &test).is_some() {
                pairs.push((test, format!("{folder}{stem}.test.jadpo")));
            }
            for (from, to_path) in &pairs {
                if file_at(db, project, to_path).is_some() {
                    return Err(refused(
                        format!("`{to_path}` already exists"),
                        format!(
                            "A declaration lives in the file named after it, so renaming `{name}` moves `{from}` to `{to_path}`, which is taken."
                        ),
                        "Choose another name, or move that file first".to_string(),
                    ));
                }
            }
            moves = pairs;
        }
    }

    // The registry's line.
    let registry = crate::graph::registry(db, project);
    let now = identity_now(graph, registry, node);
    let after = with_own(&now, &new_own);
    let born = key.name().to_string();
    let word_kind = kind.word();
    let mut lines = project.identities(db).clone();
    let had = lines.remove(&format!("{word_kind} {now}"));
    let parent_names = owner.map(|owner| {
        (
            identity_now(graph, registry, owner),
            owner.key.name().to_string(),
        )
    });
    // A line only when the name after doesn't already lead back to the born
    // name: renamed back, or a member under its owner's name again.
    let derived = Registry::from_lines(&lines).born(
        kind,
        &after,
        parent_names
            .as_ref()
            .map(|(now, born)| (now.as_str(), born.as_str())),
    );
    let needed = derived != born;
    // A name another of its kind was born with is in use too: stored
    // records and the review still know that one by it, so the name now
    // and the born names stay apart.
    if needed
        && let Some(other) = graph
            .nodes()
            .iter()
            .find(|other| other.kind == kind && other.key != *key && other.key.name() == derived)
    {
        return Err(Box::new(
            Diagnostic::new(
                &rule::CLI_RENAME_ARGUMENTS,
                format!("`{new_own}` is the name `{}` was born with", other.name),
            )
            .reason(format!(
                "Stored records and the review know `{}` by the name it was born with, so `{name}` can't take `{new_own}` while it keeps that identity.",
                other.name
            ))
            .next(format!("Choose another name, then run `{again}`")),
        ));
    }
    if needed {
        lines.insert(format!("{word_kind} {after}"), born.clone());
    }
    // A type or entity carries its members' lines, a library its
    // functions' and constants'.
    let follows: &[&str] = match kind {
        Kind::Entity | Kind::Type => MEMBERS,
        Kind::Library => LIBRARY_MEMBERS,
        _ => &[],
    };
    if !follows.is_empty() {
        let prefix = format!("{now}.");
        let moved: Vec<(String, String)> = lines
            .iter()
            .filter(|(line, _)| {
                line.split_once(' ').is_some_and(|(kind, name)| {
                    follows.contains(&kind) && name.starts_with(&prefix)
                })
            })
            .map(|(line, born)| (line.clone(), born.clone()))
            .collect();
        for (line, born) in moved {
            lines.remove(&line);
            let (kind, name) = line.split_once(' ').unwrap_or_default();
            lines.insert(format!("{kind} {after}.{}", &name[prefix.len()..]), born);
        }
    }
    let shown = |name: &str| -> String {
        if owner.is_some() {
            name.rsplit('.').next().unwrap_or(name).to_string()
        } else {
            name.to_string()
        }
    };
    let subject = format!("{word_kind} {born}");
    let line = match (had.is_some(), needed) {
        (false, _) => Row {
            mark: Mark::Added,
            subject,
            note: format!("now {}", shown(&after)),
        },
        (true, true) => Row {
            mark: Mark::Changed,
            subject,
            note: format!("now {}, was {}", shown(&after), shown(&now)),
        },
        (true, false) => Row {
            mark: Mark::Removed,
            subject,
            note: "back to its born name".to_string(),
        },
    };

    Ok(Plan {
        key: key.clone(),
        kind,
        word,
        name,
        to,
        own,
        new_own,
        edits,
        moves,
        lines,
        line,
        kept: kept(db, project, node, owner),
    })
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn file_at(db: &dyn Db, project: Project, path: &str) -> Option<jadpo_db::File> {
    project
        .files(db)
        .iter()
        .copied()
        .find(|file| file.path(db) == path)
}

/// Whether `other`'s name holds the words of `name`, in order.
fn shares_words(name: &str, other: &str) -> bool {
    let words: Vec<String> = jadpo_db::name_words(name)
        .iter()
        .map(|word| word.to_lowercase())
        .collect();
    let theirs: Vec<String> = jadpo_db::name_words(other)
        .iter()
        .map(|word| word.to_lowercase())
        .collect();
    !words.is_empty() && theirs.windows(words.len()).any(|window| window == words)
}

/// The most names the `in use` list shows, as `semantic.duplicate_declaration`'s does.
const IN_USE: usize = 12;

fn limited(mut rows: Vec<(String, String)>) -> Vec<(String, String)> {
    rows.sort();
    rows.dedup();
    if rows.len() > IN_USE {
        let more = rows.len() - (IN_USE - 1);
        rows.truncate(IN_USE - 1);
        rows.push((format!("and {more} more"), String::new()));
    }
    rows
}

/// The refusal for a member's new name that its owner already has.
fn member_in_use(
    graph: &Graph,
    node: &Node,
    owner: &Node,
    name: &str,
    to: &str,
    new_own: &str,
) -> Option<Box<Diagnostic>> {
    let siblings: Vec<&Node> = graph
        .nodes()
        .iter()
        .filter(|other| {
            other.parent.as_ref() == Some(&owner.key)
                && other.key != node.key
                && (member(other.kind) || owner.kind == Kind::Library)
        })
        .collect();
    let taken = siblings.iter().find(|other| other.name == to)?;
    let file = |other: &Node| {
        other
            .place
            .as_ref()
            .map_or(String::new(), |place| place.file.clone())
    };
    let rows = siblings
        .iter()
        .filter(|other| {
            let theirs = other.name.rsplit('.').next().unwrap_or(&other.name);
            shares_words(new_own, theirs)
        })
        .map(|other| (other.name.clone(), file(other)))
        .collect();
    let what = taken.kind.word();
    let of = if owner.kind == Kind::Library {
        format!("the library `{}`", owner.name)
    } else {
        format!("`{}`", owner.name)
    };
    Some(Box::new(
        Diagnostic::new(
            &rule::CLI_RENAME_ARGUMENTS,
            format!("`{new_own}` is already {} {what} of {of}", a_or_an(what)),
        )
        .reason(format!(
            "A rename keeps each name to one thing, so `{name}` can't be called `{new_own}` while `{}` is.",
            taken.name
        ))
        .next(format!(
            "Choose another name, then run `jadpo rename {} <new-name>`",
            shell(name)
        ))
        .in_use(limited(rows)),
    ))
}

/// The refusal for a declaration's new name that another declaration it
/// has to differ from already has: in the domain for a private function
/// or type, across the project for everything else.
fn declaration_in_use(
    db: &dyn Db,
    project: Project,
    node: &Node,
    name: &str,
    new_own: &str,
    again: &str,
) -> Option<Box<Diagnostic>> {
    let mine = symbol_of(db, project, node);
    let scope = mine.as_ref().and_then(|symbol| symbol.scope.clone());
    let symbols = jadpo_db::symbols(db, project);
    let ours = |symbol: &&Symbol| symbol.provided.is_none() && symbol.ownership.package().is_none();
    let clashes =
        |symbol: &Symbol| scope.is_none() || symbol.scope.is_none() || symbol.scope == scope;
    let taken = symbols
        .declarations
        .get(new_own)?
        .iter()
        .filter(ours)
        .find(|symbol| clashes(symbol))?
        .clone();
    let rows = symbols
        .declarations
        .iter()
        .filter(|(other, _)| shares_words(new_own, other))
        .flat_map(|(_, found)| found.iter())
        .filter(ours)
        .filter(|symbol| {
            mine.as_ref()
                .is_none_or(|mine| !(symbol.path == mine.path && symbol.index == mine.index))
        })
        .map(|symbol| (symbol.name.clone(), symbol.path.clone()))
        .collect();
    let unique = match &scope {
        Some(domain) if taken.scope.as_deref() == Some(domain) => {
            format!("and a private name is unique within `{domain}`")
        }
        _ => "and names are unique across the project".to_string(),
    };
    Some(Box::new(
        Diagnostic::new(
            &rule::CLI_RENAME_ARGUMENTS,
            format!("`{new_own}` is already declared"),
        )
        .reason(format!(
            "`{new_own}` is the {} in `{}`, {unique}, so `{name}` can't take it.",
            taken.kind.word(),
            taken.path
        ))
        .next(format!("Choose another name, then run `{}`", again))
        .in_use(limited(rows)),
    ))
}

/// The fields copied from field `key` (`pick:`), and from those, in turn.
fn copies(graph: &Graph, key: &Key) -> Vec<Key> {
    let mut found: Vec<Key> = Vec::new();
    let mut frontier: BTreeSet<Key> = BTreeSet::from([key.clone()]);
    while !frontier.is_empty() {
        let next: BTreeSet<Key> = graph
            .nodes()
            .iter()
            .filter(|node| {
                matches!(
                    &node.detail,
                    Detail::Field { copied_from: Some(source), .. } if frontier.contains(source)
                )
            })
            .map(|node| node.key.clone())
            .filter(|copy| copy != key && !found.contains(copy))
            .collect();
        found.extend(next.iter().cloned());
        frontier = next;
    }
    found
}

/// Each place `keys` are written, by file, outside migration folders and
/// installed packages.
fn references(db: &dyn Db, project: Project, keys: &[Key]) -> BTreeMap<String, Vec<Edit>> {
    let mut out: BTreeMap<String, Vec<Edit>> = BTreeMap::new();
    for key in keys {
        for reference in node_references(db, project, key) {
            if reference.path.starts_with("database/") {
                continue;
            }
            let Some(file) = file_at(db, project, &reference.path) else {
                continue;
            };
            if jadpo_db::location(db, project, file)
                .ownership
                .package()
                .is_some()
            {
                continue;
            }
            let text = file.text(db);
            let line = text[..reference.start as usize].matches('\n').count() as u32 + 1;
            let in_use = crate::tree::source(db, file).is_some_and(|source| {
                source
                    .syntax()
                    .token_at_offset(reference.start.into())
                    .right_biased()
                    .is_some_and(|token| {
                        token.parent_ancestors().any(|node| node.kind() == USE_DECL)
                    })
            });
            out.entry(reference.path.clone()).or_default().push(Edit {
                start: reference.start,
                end: reference.end,
                line,
                shown: !in_use,
            });
        }
    }
    for list in out.values_mut() {
        list.sort_by_key(|edit| (edit.start, edit.end));
        list.dedup_by_key(|edit| (edit.start, edit.end));
    }
    out
}

/// What stored data the node's rename keeps: an entity's table, an
/// entity field's column.
fn kept(db: &dyn Db, project: Project, node: &Node, owner: Option<&Node>) -> Option<String> {
    let model = crate::data::data_model(db, project);
    match node.kind {
        Kind::Entity => model
            .entities
            .iter()
            .find(|entity| entity.born == node.key.name())
            .map(|entity| format!("table {}", entity.table)),
        Kind::Field => {
            let owner = owner?;
            if owner.kind != Kind::Entity {
                return None;
            }
            let entity = model
                .entities
                .iter()
                .find(|entity| entity.born == owner.key.name())?;
            let own = node
                .key
                .name()
                .strip_prefix(&format!("{}.", owner.key.name()))?;
            let field = entity.fields.iter().find(|field| field.born == own)?;
            Some(match field.compound {
                Some(_) => format!("the columns of {}.{}", entity.table, field.born),
                None => format!("column {}.{}", entity.table, field.born),
            })
        }
        _ => None,
    }
}

/// Each error's rule and headline, counted.
pub(crate) fn headlines(problems: &[Diagnostic]) -> BTreeMap<(String, String), usize> {
    let mut out: BTreeMap<(String, String), usize> = BTreeMap::new();
    for problem in problems {
        *out.entry((problem.rule.id.to_string(), problem.message.clone()))
            .or_default() += 1;
    }
    out
}

/// Makes the plan's edits in `loaded`, moves the files, writes the
/// registry and the records the rename puts out of date, then fixes
/// imports, formats and checks; undone, with the errors it would add, when
/// the result has an error the project didn't have.
pub fn apply(loaded: &mut Loaded, plan: &Plan) -> Result<Renamed, Stopped> {
    let project = loaded.project;
    let before_errors = headlines(&crate::check(&loaded.db, project));
    let before_records = record_states(&loaded.db, project);
    let text_of =
        |loaded: &Loaded, path: &str| loaded.file(path).map(|file| file.text(&loaded.db).clone());
    let mut paths: BTreeSet<String> = plan.edits.keys().cloned().collect();
    for (from, to) in &plan.moves {
        paths.insert(from.clone());
        paths.insert(to.clone());
    }
    let mut before: BTreeMap<String, Option<String>> = BTreeMap::new();
    for path in &paths {
        before.insert(path.clone(), text_of(loaded, path));
    }
    let mut records_before: BTreeMap<String, Option<String>> = BTreeMap::new();
    records_before.insert(
        jadpo_db::IDENTITIES.to_string(),
        loaded.record(jadpo_db::IDENTITIES),
    );

    // 1. Every place it is written.
    for (path, edits) in &plan.edits {
        let text = text_of(loaded, path).unwrap_or_default();
        let replacements = edits
            .iter()
            .map(|edit| (edit.start, edit.end, plan.new_own.clone()))
            .collect();
        loaded.set_text(path, crate::apply_edits(&text, replacements));
    }

    // 2. The files named after it.
    for (from, to) in &plan.moves {
        if let Some(text) = text_of(loaded, from) {
            loaded.remove(from);
            loaded.set_text(to, text);
        }
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
        .map(|(from, to)| (to.as_str(), from.as_str()))
        .collect();
    format_as_before(loaded, &present, &before, &moved_from);

    // 4. The registry, then each record the rename put out of date.
    loaded.set_record(
        jadpo_db::IDENTITIES,
        Some(jadpo_db::write_identities(&plan.lines)),
    );
    let rewritten = rewrite_records(loaded, &before_records, &mut records_before);

    // 5. Checked: a rename never adds an error.
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
        return Err(Stopped {
            problems: added,
            sources,
        });
    }

    // What changed, file by file: the declaration's first.
    let declared = plan
        .moves
        .first()
        .map(|(from, _)| from.clone())
        .or_else(|| {
            let graph = crate::graph::graph(&loaded.db, project);
            graph
                .get(&plan.key)
                .and_then(|node| node.place.as_ref())
                .map(|place| place.file.clone())
        });
    let mut order: Vec<String> = Vec::new();
    if let Some(declared) = &declared {
        order.push(declared.clone());
    }
    for (from, to) in &plan.moves {
        for path in [from, to] {
            if !order.contains(path) {
                order.push(path.clone());
            }
        }
    }
    for path in &paths {
        if !order.contains(path) {
            order.push(path.clone());
        }
    }
    let file_name = |path: &str| path.rsplit('/').next().unwrap_or(path).to_string();
    let mut files = Vec::new();
    for path in order {
        let was = before.get(&path).cloned().flatten();
        let now = after.get(&path).cloned().flatten();
        if was == now {
            continue;
        }
        let mut rows = Vec::new();
        if let Some((_, to)) = plan.moves.iter().find(|(from, _)| *from == path) {
            rows.push(Row {
                mark: Mark::Removed,
                subject: file_name(&path),
                note: format!("file, renamed {}", file_name(to)),
            });
        }
        let edited = match moved_from.get(path.as_str()) {
            Some(from) => {
                rows.push(Row {
                    mark: Mark::Added,
                    subject: file_name(&path),
                    note: format!("file, was {}", file_name(from)),
                });
                plan.edits.get(*from)
            }
            None if plan.moves.iter().any(|(from, _)| *from == path) => None,
            None => plan.edits.get(&path),
        };
        let mut seen = BTreeSet::new();
        for edit in edited.into_iter().flatten().filter(|edit| edit.shown) {
            if seen.insert(edit.line) {
                rows.push(Row {
                    mark: Mark::Changed,
                    subject: format!("line {}", edit.line),
                    note: format!("{}, was {}", plan.new_own, plan.own),
                });
            }
        }
        files.push(FileChange {
            path,
            before: was,
            after: now,
            rows,
        });
    }
    let mut records = false;
    for path in rewritten
        .iter()
        .chain(std::iter::once(&jadpo_db::IDENTITIES.to_string()))
    {
        let was = records_before.get(path).cloned().flatten();
        let now = records_after.get(path).cloned().flatten();
        if was == now && path != jadpo_db::IDENTITIES {
            continue;
        }
        let rows = if path == jadpo_db::IDENTITIES {
            vec![plan.line.clone()]
        } else {
            records = true;
            vec![Row {
                mark: Mark::Changed,
                subject: "record".to_string(),
                note: "brought up to date".to_string(),
            }]
        };
        files.push(FileChange {
            path: path.clone(),
            before: was,
            after: now,
            rows,
        });
    }
    Ok(Renamed {
        files,
        errors_left: problems.len(),
        records,
    })
}

/// Each record's state, by path, to tell a record a change puts out of
/// date from one that already was.
pub(crate) fn record_states(
    db: &dyn Db,
    project: Project,
) -> BTreeMap<String, crate::records::State> {
    crate::records::standings(db, project)
        .iter()
        .map(|standing| (standing.path.clone(), standing.state))
        .collect()
}

/// Formats each of `present` that was formatted before the change, as
/// `jadpo fmt` would; `moved_from` gives a moved file's path before, so its
/// old text says whether it was.
pub(crate) fn format_as_before(
    loaded: &mut Loaded,
    present: &[String],
    before: &BTreeMap<String, Option<String>>,
    moved_from: &BTreeMap<&str, &str>,
) {
    let layout_project = loaded.project.layout(&loaded.db) == jadpo_db::Layout::Project;
    for path in present {
        let Some(after) = loaded.file(path).map(|file| file.text(&loaded.db).clone()) else {
            continue;
        };
        let was = match moved_from.get(path.as_str()) {
            Some(from) => before.get(*from).cloned().flatten(),
            None => before.get(path).cloned().flatten(),
        };
        let canonical = |text: &str| crate::setting_move::canonical(path, text, layout_project);
        let formatted = match &was {
            Some(was) => canonical(was).as_deref() == Some(was.as_str()),
            None => true,
        };
        if formatted && let Some(text) = canonical(&after) {
            loaded.set_text(path, text);
        }
    }
}

/// Writes again each record, other than the registry, that the change put
/// out of date, as `jadpo records update` writes it, keeping each one's
/// text before in `records_before`; the paths it wrote.
pub(crate) fn rewrite_records(
    loaded: &mut Loaded,
    before_records: &BTreeMap<String, crate::records::State>,
    records_before: &mut BTreeMap<String, Option<String>>,
) -> Vec<String> {
    let mut rewritten: Vec<String> = Vec::new();
    let standings: Vec<crate::records::Standing> =
        crate::records::standings(&loaded.db, loaded.project).to_vec();
    for standing in standings {
        if standing.path == jadpo_db::IDENTITIES
            || !standing.state.is_problem()
            || before_records
                .get(&standing.path)
                .is_some_and(|state| state.is_problem())
        {
            continue;
        }
        for (path, fix) in &standing.fixes {
            records_before
                .entry(path.clone())
                .or_insert_with(|| loaded.record(path));
            match fix {
                crate::records::Fix::Write(text) => loaded.set_record(path, Some(text.clone())),
                crate::records::Fix::Remove => loaded.set_record(path, None),
            }
            rewritten.push(path.clone());
        }
    }
    rewritten
}

/// The errors of `problems` the project didn't have, by rule and headline,
/// counting each the project had as many times as it had it.
pub(crate) fn added_errors(
    mut had: BTreeMap<(String, String), usize>,
    problems: &[Diagnostic],
) -> Vec<Diagnostic> {
    let mut added: Vec<Diagnostic> = Vec::new();
    for problem in problems {
        let key = (problem.rule.id.to_string(), problem.message.clone());
        match had.get_mut(&key) {
            Some(count) if *count > 0 => *count -= 1,
            _ => added.push(problem.clone()),
        }
    }
    added
}

/// Puts back the texts and records [`apply`] changed, for the editor,
/// which applies the edit itself.
pub fn restore(loaded: &mut Loaded, renamed: &Renamed) {
    for change in &renamed.files {
        if change.path.ends_with(".jadpo_record") {
            loaded.set_record(&change.path, change.before.clone());
        } else {
            match &change.before {
                Some(text) => loaded.set_text(&change.path, text.clone()),
                None => loaded.remove(&change.path),
            }
        }
    }
}
