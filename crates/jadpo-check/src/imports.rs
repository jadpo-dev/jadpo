//! Imports (area 1, "Imports", decided 2026-10-14; built in chunk 3.5).
//!
//! Every name a file uses that it doesn't declare comes in on a `use`
//! line, the file's own domain's included: `use shared/types { Pence }`
//! brings in a name from the folder holding its file, and `use menu` a
//! domain, whose actions, queries and fixtures are still called through
//! it (`menu.price_item(...)`). Built-ins are never imported, and a test
//! file sees everything its subject file declares. Loose files have no
//! layout, so no folders to import from.
//!
//! [`import_problems`] reads the file's own tree and, one name at a time,
//! the symbol table (docs/compiler-runtime.md §2.4). What a file uses is
//! read from its text, as written: an upper-case name is a type, an
//! entity, a failure or an event; a bare call names a function, action,
//! query or fixture; and the name before a dot owns what follows it, a
//! domain, a library or a connection. So a file's imports depend only on
//! its own text and the entries of the names it writes, never on another
//! file's text, and editing a body elsewhere rechecks nothing here.
//!
//! Each problem's edit is the one-step fix, and [`import_edits`] gives
//! every fix at once that `jadpo fmt` applies: it adds a missing import,
//! removes an unused one and moves a name to the folder it is in. The
//! formatter then sorts the lines.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{
    Db, File, Folder, ItemKind, Layout, Location, Ownership, Place, Project, RootFile, Symbol,
    location,
};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::SyntaxKind::*;
use jadpo_syntax::ast::AstNode;
use jadpo_syntax::{SyntaxNode, SyntaxToken};

use crate::case::starts_upper;
use crate::lookups::{Lookup, NameView};
use crate::std_lib::StdNames;
use crate::tree;

/// The rules whose edits `jadpo fmt` applies.
pub const FIXED_BY_FORMAT: [&str; 5] = [
    "import.duplicate",
    "import.missing",
    "import.unknown",
    "import.unneeded",
    "import.unused",
];

/// How a name is written, which says what it can be.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Role {
    /// An upper-case name where it stands first: a type, an entity, a
    /// failure or an event, `Pence` or `Todo.get`.
    Type,
    /// A bare call: `price_lines(...)`.
    Call,
    /// The name before a dot: `menu.price_item(...)`, `money.round(...)`.
    Owner,
}

/// The first place a file writes a name that may come from elsewhere.
#[derive(Clone, Debug)]
struct Written {
    name: String,
    role: Role,
    start: u32,
    end: u32,
}

/// One `use` line, read from the tree.
#[derive(Clone, Debug)]
pub struct UseLine {
    /// The `use` keyword's start and the end of the line's last token.
    pub start: u32,
    pub end: u32,
    /// The path's names, each with its byte range.
    pub path: Vec<(String, u32, u32)>,
    pub names: Option<UseNames>,
}

/// The braces of a `use` line.
#[derive(Clone, Debug)]
pub struct UseNames {
    /// The byte ranges of `{` and of `}`, when written.
    pub open: (u32, u32),
    pub close: Option<(u32, u32)>,
    /// Each name with its byte range, in written order.
    pub entries: Vec<(String, u32, u32)>,
}

impl UseLine {
    /// The path as written: `shared/types`.
    pub fn folder(&self) -> String {
        self.path
            .iter()
            .map(|(name, _, _)| name.as_str())
            .collect::<Vec<_>>()
            .join("/")
    }

    fn path_span(&self) -> (u32, u32) {
        match (self.path.first(), self.path.last()) {
            (Some(first), Some(last)) => (first.1, last.2),
            _ => (self.start, self.end),
        }
    }
}

/// The `use` lines of a file, in source order.
pub fn use_lines(source: &SyntaxNode) -> Vec<UseLine> {
    source
        .children()
        .filter(|child| child.kind() == USE_DECL)
        .map(|line| {
            let span = tree::node_span("", &line);
            let path = line
                .children()
                .find(|child| child.kind() == USE_PATH)
                .into_iter()
                .flat_map(|path| path.children())
                .filter(|child| child.kind() == NAME_REF)
                .filter_map(|name| named(&name))
                .collect();
            let names = line
                .children()
                .find(|child| child.kind() == USE_NAMES)
                .map(|list| {
                    let brace = |kind| {
                        list.children_with_tokens()
                            .filter_map(|element| element.into_token())
                            .find(|token| token.kind() == kind)
                            .map(|token| range(&token))
                    };
                    UseNames {
                        open: brace(L_BRACE).unwrap_or((span.start, span.start)),
                        close: brace(R_BRACE),
                        entries: list
                            .children()
                            .filter(|child| child.kind() == NAME_REF)
                            .filter_map(|name| named(&name))
                            .collect(),
                    }
                });
            UseLine {
                start: span.start,
                end: span.end,
                path,
                names,
            }
        })
        .collect()
}

fn range(token: &SyntaxToken) -> (u32, u32) {
    (
        u32::from(token.text_range().start()),
        u32::from(token.text_range().end()),
    )
}

fn named(node: &SyntaxNode) -> Option<(String, u32, u32)> {
    let token = node
        .children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == IDENT)?;
    let (start, end) = range(&token);
    Some((token.text().to_string(), start, end))
}

/// Every name the file writes that may come from elsewhere, at its first
/// use, in source order.
fn written(source: &SyntaxNode) -> Vec<Written> {
    let mut seen: BTreeSet<(String, Role)> = BTreeSet::new();
    let mut out = Vec::new();
    for node in source.descendants() {
        let found = match node.kind() {
            NAME_REF => name_ref_use(&node),
            MEMBER_KEY => member_key_use(&node),
            _ => None,
        };
        if let Some((name, role, start, end)) = found
            && seen.insert((name.clone(), role))
        {
            out.push(Written {
                name,
                role,
                start,
                end,
            });
        }
    }
    out
}

fn name_ref_use(node: &SyntaxNode) -> Option<(String, Role, u32, u32)> {
    let (name, start, end) = named(node)?;
    let upper = starts_upper(&name);
    let parent = node.parent()?;
    let role = match parent.kind() {
        USE_PATH | USE_NAMES => return None,
        // Only a path's first name names something; the rest are members.
        PATH => {
            if parent.first_child().as_ref() != Some(node) || !upper {
                return None;
            }
            Role::Type
        }
        // `a.b`: `a` owns `b`, which is a member.
        FIELD_EXPR => {
            if parent.first_child().as_ref() != Some(node) {
                return None;
            }
            if upper { Role::Type } else { Role::Owner }
        }
        CALL_EXPR if !upper => Role::Call,
        _ if upper => Role::Type,
        _ => return None,
    };
    Some((name, role, start, end))
}

/// A dotted member key, `Todo.priority: ...`: its first name.
fn member_key_use(node: &SyntaxNode) -> Option<(String, Role, u32, u32)> {
    let names: Vec<SyntaxNode> = node
        .children()
        .filter(|child| child.kind() == NAME)
        .collect();
    if names.len() < 2 {
        return None;
    }
    let (name, start, end) = named(&names[0])?;
    let role = if starts_upper(&name) {
        Role::Type
    } else {
        Role::Owner
    };
    Some((name, role, start, end))
}

/// The folder a `use` line names to bring `symbol` in: `<domain>/<folder>`,
/// `shared/<folder>` or a root file. `None` for what nothing imports: a
/// test file's declarations, and the rest of the root folders'.
pub fn folder_of(symbol: &Symbol) -> Option<String> {
    if symbol.test || symbol.library.is_some() {
        return None;
    }
    match (&symbol.ownership, &symbol.place) {
        (Ownership::Domain(domain), Place::Folder(folder)) => {
            Some(format!("{domain}/{}", folder.name()))
        }
        (Ownership::Shared, Place::Folder(folder)) => Some(format!("shared/{}", folder.name())),
        (_, Place::RootFile(root)) => Some(root_name(*root).to_string()),
        _ => None,
    }
}

fn root_name(root: RootFile) -> &'static str {
    root.file_name().trim_end_matches(".jadpo")
}

/// What a written name brings in from elsewhere.
#[derive(Clone, Debug)]
enum Need {
    /// A declaration, by its symbol.
    Name(Box<Symbol>),
    /// A domain, for calls through it.
    Domain(String),
    /// A connection declared in `app.jadpo` (area 11).
    Connection(String),
}

/// Where a `use` line's path points.
enum Target {
    Domain(String),
    Folder(String),
}

/// The file being checked, and how to read names from where it is.
struct Here<'a> {
    path: &'a str,
    text: &'a str,
    location: &'a Location,
    /// For a test file, its subject's path.
    subject: Option<String>,
    table: &'a dyn NameView,
    std: &'static StdNames,
    lines: Vec<UseLine>,
    /// Where a first `use` line goes.
    top: u32,
}

/// The import problems of one file.
#[salsa::tracked(returns(ref))]
pub fn import_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let path = file.path(db);
    let lines = use_lines(source.syntax());
    if project.layout(db) == Layout::Loose {
        return lines
            .iter()
            .map(|line| loose(path, file.text(db), line))
            .collect();
    }
    let location = location(db, project, file);
    // A migration's decisions name what changed (`Todo.rank: { ... }`) in
    // a record `jadpo migrate new` writes; they import nothing.
    if location.place == Place::Migration {
        return Vec::new();
    }
    let lookup = Lookup::new(db, project);
    let here = Here {
        path,
        text: file.text(db),
        location,
        subject: location.test.then(|| path.replace(".test.jadpo", ".jadpo")),
        table: &lookup,
        std: StdNames::get(),
        top: top(source.syntax()),
        lines,
    };
    here.problems(&written(source.syntax()))
}

/// Every edit `jadpo fmt` makes to a file's imports, from the problems
/// [`import_problems`] gives: the edits of each, leaving out any that
/// overlap one already taken. Applying them and checking again converges
/// in a few rounds; the formatter then sorts the lines.
pub fn import_edits(problems: &[Diagnostic]) -> Vec<(u32, u32, String)> {
    let mut taken: Vec<(u32, u32, String)> = Vec::new();
    for problem in problems {
        if !FIXED_BY_FORMAT.contains(&problem.rule.id) {
            continue;
        }
        let Some(next) = &problem.next else { continue };
        let edits: Vec<(u32, u32, String)> = next
            .edits
            .iter()
            .map(|edit| (edit.span.start, edit.span.end, edit.replacement.clone()))
            .collect();
        let clashes = edits.iter().any(|(start, end, _)| {
            taken
                .iter()
                .any(|(s, e, _)| clash((*start, *end), (*s, *e)))
        });
        if !clashes {
            taken.extend(edits);
        }
    }
    taken
}

/// Where a file's first `use` line goes: after the comments that open the
/// file up to a blank line, before the comments attached to its first
/// declaration.
fn top(source: &SyntaxNode) -> u32 {
    let Some(first) = source
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| !token.kind().is_trivia())
    else {
        return 0;
    };
    let mut at = u32::from(first.text_range().start());
    let mut token = first.prev_token();
    while let Some(trivia) = token {
        match trivia.kind() {
            WHITESPACE if trivia.text().matches('\n').count() >= 2 => break,
            WHITESPACE => {}
            COMMENT | DOC_COMMENT => at = u32::from(trivia.text_range().start()),
            _ => break,
        }
        token = trivia.prev_token();
    }
    at
}

fn loose(path: &str, text: &str, line: &UseLine) -> Diagnostic {
    let folder = line.folder();
    let (start, end) = line.path_span();
    Diagnostic::new(
        &rule::IMPORT_UNKNOWN,
        format!("`{folder}` isn't a folder here"),
    )
    .at(Span::new(path, start, end), "no layout")
    .reason("These files are checked without an `app.jadpo` above them, so they have no layout and no folders to import from: every name in them is seen without a `use` line.")
    .next("Remove this line, or check the file inside its project")
    .edit(
        {
            let (start, end) = whole_line(text, line);
            Span::new(path, start, end)
        },
        "",
    )
}

/// A whole `use` line, from the start of its line to the start of the
/// next.
fn whole_line(text: &str, line: &UseLine) -> (u32, u32) {
    let bytes = text.as_bytes();
    let mut start = line.start as usize;
    while start > 0 && matches!(bytes[start - 1], b' ' | b'\t') {
        start -= 1;
    }
    let end = text[line.end as usize..]
        .find('\n')
        .map_or(text.len(), |i| line.end as usize + i + 1);
    (start as u32, end as u32)
}

/// Whether two edits can't both be applied: they overlap, two insertions
/// are at one place, or one inserts inside what the other replaces.
fn clash(a: (u32, u32), b: (u32, u32)) -> bool {
    let inside = |at: u32, (s, e): (u32, u32)| s < at && at < e;
    (a.0 < b.1 && b.0 < a.1)
        || (a.0 == a.1 && b.0 == b.1 && a.0 == b.0)
        || (a.0 == a.1 && inside(a.0, b))
        || (b.0 == b.1 && inside(b.0, a))
}

fn did_you_mean(near: Option<&str>) -> String {
    near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
}

impl Here<'_> {
    fn span(&self, start: u32, end: u32) -> Span {
        Span::new(self.path, start, end)
    }

    fn own_domain(&self) -> Option<&str> {
        self.location.ownership.domain()
    }

    /// Whether `symbol` is declared in this file, or in the subject of a
    /// test file.
    fn declared_here(&self, symbol: &Symbol) -> bool {
        symbol.path == self.path || self.subject.as_deref() == Some(symbol.path.as_str())
    }

    /// Whether this file may use `symbol`: what resolution reports as
    /// private, or as domain code used from `shared/`, gets no import.
    fn may_use(&self, symbol: &Symbol) -> bool {
        let ownership = &self.location.ownership;
        if *ownership == Ownership::Shared {
            return symbol.ownership.domain().is_none();
        }
        let outside = *ownership == Ownership::Root
            && (self.path.starts_with("tests/") || symbol.kind == ItemKind::Fixture);
        match &symbol.ownership {
            Ownership::Domain(_) => symbol.visible_from(ownership) && !(outside && !symbol.public),
            _ => true,
        }
    }

    /// What a written name brings in, if it names something in the
    /// project.
    fn need(&self, written: &Written) -> Option<Need> {
        let name = written.name.as_str();
        match written.role {
            Role::Type => {
                if self.std.is_type(name) || self.table.is_package(name) {
                    return None;
                }
                let symbol = self
                    .table
                    .declared(name)
                    .iter()
                    .find(|symbol| symbol.library.is_none())?;
                Some(Need::Name(Box::new(symbol.clone())))
            }
            // A bare call reaches only the file's own domain; one into
            // another domain is resolution's to report.
            Role::Call => {
                let symbol = self.table.declared(name).iter().find(|symbol| {
                    symbol.library.is_none()
                        && symbol.ownership == self.location.ownership
                        && matches!(
                            symbol.kind,
                            ItemKind::Function
                                | ItemKind::Action
                                | ItemKind::Query
                                | ItemKind::Fixture
                        )
                })?;
                Some(Need::Name(Box::new(symbol.clone())))
            }
            Role::Owner => {
                if self.std.reserved(name) {
                    return None;
                }
                if self.table.is_domain(name) {
                    return Some(Need::Domain(name.to_string()));
                }
                if let Some(library) = self.table.library(name) {
                    return Some(Need::Name(Box::new(library.clone())));
                }
                if self.table.is_connection(name) {
                    return Some(Need::Connection(name.to_string()));
                }
                None
            }
        }
    }

    fn problems(&self, written: &[Written]) -> Vec<Diagnostic> {
        let needs: Vec<(&Written, Need)> = written
            .iter()
            .filter_map(|w| Some((w, self.need(w)?)))
            .collect();
        // What the file uses, whether or not it needs an import for it.
        let used_names: BTreeSet<&str> = needs
            .iter()
            .filter_map(|(_, need)| match need {
                Need::Name(symbol) => Some(symbol.name.as_str()),
                Need::Connection(name) => Some(name.as_str()),
                Need::Domain(_) => None,
            })
            .collect();
        let used_domains: BTreeSet<&str> = needs
            .iter()
            .filter_map(|(_, need)| match need {
                Need::Domain(domain) => Some(domain.as_str()),
                _ => None,
            })
            .collect();
        // A name the file writes that names nothing is resolution's to
        // report; while one does, it may be a misspelling of an import, so
        // no import is called unused and `jadpo fmt` removes none.
        let unresolved = written.iter().any(|w| match w.role {
            Role::Type => {
                !self.std.is_type(&w.name)
                    && !self.table.is_package(&w.name)
                    && self.table.declared(&w.name).is_empty()
            }
            Role::Call => {
                self.table.declared(&w.name).is_empty() && self.std.namespace_of(&w.name).is_none()
            }
            Role::Owner => false,
        });
        let mut out = Vec::new();
        let mut imported: BTreeMap<String, (u32, u32)> = BTreeMap::new();
        let mut domains: BTreeMap<String, usize> = BTreeMap::new();
        let mut folders: BTreeMap<String, usize> = BTreeMap::new();
        for (index, line) in self.lines.iter().enumerate() {
            let target = match self.target(line) {
                Ok(target) => target,
                Err(problem) => {
                    // A line that names no folder still says which names
                    // the file meant to import.
                    for (name, start, end) in line.names.iter().flat_map(|n| &n.entries) {
                        imported.entry(name.clone()).or_insert((*start, *end));
                    }
                    out.push(*problem);
                    continue;
                }
            };
            match target {
                Target::Domain(domain) => {
                    if let Some(&first) = domains.get(&domain) {
                        out.push(self.duplicate_line(index, first));
                    } else if self.own_domain() == Some(domain.as_str()) {
                        out.push(self.own_domain_line(index, &domain));
                    } else if !used_domains.contains(domain.as_str()) && !unresolved {
                        out.push(self.unused_domain(index, &domain));
                    }
                    domains.entry(domain).or_insert(index);
                }
                Target::Folder(folder) => {
                    let names = line.names.as_ref().map_or(&[][..], |n| &n.entries[..]);
                    if let Some(&first) = folders.get(&folder) {
                        for (name, start, end) in names {
                            imported.entry(name.clone()).or_insert((*start, *end));
                        }
                        out.push(self.duplicate_line(index, first));
                        continue;
                    }
                    folders.insert(folder.clone(), index);
                    if names.is_empty() {
                        out.push(self.empty_line(index, &folder));
                        continue;
                    }
                    for (entry, (name, start, end)) in names.iter().enumerate() {
                        let span = (*start, *end);
                        if let Some(first) = imported.get(name) {
                            out.push(self.duplicate_name(index, entry, name, *first));
                            continue;
                        }
                        imported.insert(name.clone(), span);
                        if let Some(problem) =
                            self.check_name(index, entry, &folder, name, span, &used_names)
                            && !(unresolved && problem.rule.id == rule::IMPORT_UNUSED.id)
                        {
                            out.push(problem);
                        }
                    }
                }
            }
        }
        for (written, need) in &needs {
            match need {
                Need::Name(symbol) => {
                    if self.declared_here(symbol)
                        || imported.contains_key(&symbol.name)
                        || !self.may_use(symbol)
                    {
                        continue;
                    }
                    let Some(folder) = folder_of(symbol) else {
                        continue;
                    };
                    out.push(self.missing(written, &folder, Some(symbol)));
                }
                Need::Connection(name) => {
                    if !imported.contains_key(name) {
                        out.push(self.missing(written, "app", None));
                    }
                }
                Need::Domain(domain) => {
                    if self.own_domain() != Some(domain.as_str()) && !domains.contains_key(domain) {
                        out.push(self.missing_domain(written, domain));
                    }
                }
            }
        }
        out
    }

    /// What a line's path names, or why it names nothing.
    fn target(&self, line: &UseLine) -> Result<Target, Box<Diagnostic>> {
        let parts: Vec<&str> = line.path.iter().map(|(name, _, _)| name.as_str()).collect();
        let folder = line.folder();
        let (start, end) = line.path_span();
        let span = self.span(start, end);
        let roots: Vec<&str> = RootFile::ALL.iter().map(|root| root_name(*root)).collect();
        let first = parts.first().copied().unwrap_or_default();
        let domain = self.table.is_domain(first);
        let fail = |message: String, label: &str, reason: String, next: String| {
            Box::new(
                Diagnostic::new(&rule::IMPORT_UNKNOWN, message)
                    .at(span.clone(), label)
                    .reason(reason)
                    .next(next),
            )
        };
        let edit = |problem: Box<Diagnostic>, segment: usize, near: Option<&str>| match (
            near,
            line.path.get(segment),
        ) {
            (Some(near), Some((_, s, e))) => Box::new(problem.edit(self.span(*s, *e), near)),
            _ => problem,
        };
        match (parts.len(), line.names.is_some()) {
            (1, false) if domain => Ok(Target::Domain(first.to_string())),
            (1, true) if roots.contains(&first) => Ok(Target::Folder(first.to_string())),
            (2, true) if first == "shared" => {
                match Folder::from_name(parts[1]).filter(|f| Folder::SHARED.contains(f)) {
                    Some(_) => Ok(Target::Folder(folder)),
                    None => {
                        let near = nearest(parts[1], Folder::SHARED.iter().map(|f| f.name()));
                        let problem = fail(
                            format!("`{folder}` isn't a folder of `shared/`"),
                            "not a folder",
                            format!(
                                "`shared/` holds only `types/`, `libraries/` and `failures/`: what every domain may use.{}",
                                did_you_mean(near)
                            ),
                            match near {
                                Some(near) => format!("Use `shared/{near}` instead of `{folder}`"),
                                None => "Import from `shared/types`, `shared/libraries` or `shared/failures`".to_string(),
                            },
                        );
                        Err(edit(problem, 1, near))
                    }
                }
            }
            (2, true) if domain => {
                if Folder::from_name(parts[1]).is_some() {
                    return Ok(Target::Folder(folder));
                }
                let near = nearest(parts[1], Folder::ALL.iter().map(|f| f.name()));
                let folders: Vec<String> = Folder::ALL
                    .iter()
                    .map(|folder| format!("`{}`", folder.name()))
                    .collect();
                let problem = fail(
                    format!("`{folder}` isn't a folder"),
                    "not a folder",
                    format!(
                        "A domain's names come from its folders: {}.{}",
                        crate::and_list(&folders),
                        did_you_mean(near)
                    ),
                    match near {
                        Some(near) => format!("Use `{first}/{near}` instead of `{folder}`"),
                        None => format!("Name one of `{first}`'s folders"),
                    },
                );
                Err(edit(problem, 1, near))
            }
            (1, true) if domain => Err(fail(
                format!("`{first}` is a domain, not a folder"),
                "a domain",
                format!(
                    "`use {first}` brings in the domain for calls such as `{first}.<action>(...)`; its names come in by folder, such as `use {first}/types {{ ... }}`."
                ),
                format!("Import the names from the folders under `{first}/` that hold them"),
            )),
            (1 | 2, false) if first == "shared" || roots.contains(&first) || domain => Err(fail(
                format!("`use {folder}` names nothing to bring in"),
                "no names",
                format!(
                    "A folder's `use` line lists the names the file takes from it, in braces: `use {folder} {{ Name }}`."
                ),
                format!("List the names after `{folder}`, in braces"),
            )),
            _ if first == "shared" || roots.contains(&first) || domain => Err(fail(
                format!("`{folder}` isn't a folder Jadpo imports from"),
                "not a folder",
                "A `use` line names a domain (`use menu`), a domain's folder (`use menu/types { ... }`), a folder of `shared/` (`use shared/types { ... }`) or a root file (`use app { ... }`).".to_string(),
                "Name the folder that holds the names".to_string(),
            )),
            _ => {
                let all = self.table.all();
                let mut owners: Vec<String> = all.domains.iter().cloned().collect();
                owners.push("shared".to_string());
                owners.extend(roots.iter().map(|root| root.to_string()));
                let near = nearest(first, owners.iter().map(String::as_str)).map(str::to_string);
                let domains: Vec<String> =
                    all.domains.iter().map(|domain| format!("`{domain}`")).collect();
                let listed = match domains.len() {
                    0 => String::new(),
                    1 => format!(" This project's one domain is {}.", domains[0]),
                    _ => format!(" This project's domains are {}.", crate::and_list(&domains)),
                };
                let problem = fail(
                    format!("`{first}` isn't a domain"),
                    "not a domain",
                    format!(
                        "A `use` line starts with a domain, `shared` or a root file such as `app`.{listed}{}",
                        did_you_mean(near.as_deref())
                    ),
                    match &near {
                        Some(near) => format!("Use `{near}` instead of `{first}`"),
                        None => "Import from a domain, `shared/` or a root file".to_string(),
                    },
                );
                Err(edit(problem, 0, near.as_deref()))
            }
        }
    }

    /// One name on a folder's line.
    fn check_name(
        &self,
        index: usize,
        entry: usize,
        folder: &str,
        name: &str,
        (start, end): (u32, u32),
        used: &BTreeSet<&str>,
    ) -> Option<Diagnostic> {
        let span = self.span(start, end);
        if self.std.is_type(name) || self.std.reserved(name) {
            let what = if self.std.is_type(name) {
                "a built-in type"
            } else {
                "a standard-library namespace"
            };
            return Some(self.unneeded(
                index,
                entry,
                format!("`{name}` is built in"),
                format!(
                    "`{name}` is {what}: built-ins are part of the language and are never imported."
                ),
            ));
        }
        let declared = self.table.declared(name);
        if let Some(symbol) = declared.iter().find(|symbol| self.declared_here(symbol)) {
            let (message, reason) = if symbol.path == self.path {
                (
                    format!("`{name}` is declared in this file"),
                    "A file sees what it declares without importing it.".to_string(),
                )
            } else {
                let subject = symbol.path.rsplit('/').next().unwrap_or(&symbol.path);
                (
                    format!("`{name}` is declared in this test's subject"),
                    format!(
                        "A test file sees everything its subject declares without importing it, and `{name}` is in `{subject}`."
                    ),
                )
            };
            return Some(self.unneeded(index, entry, message, reason));
        }
        if declared.is_empty() {
            if self.table.is_connection(name) {
                if folder == "app" {
                    return (!used.contains(name)).then(|| self.unused_name(index, entry, name));
                }
                return Some(
                    Diagnostic::new(&rule::IMPORT_UNKNOWN, format!("`{name}` isn't in `{folder}`"))
                        .at(span, "not in this folder")
                        .reason(format!(
                            "`{name}` is a connection, declared in `app.jadpo`, so it comes in from `app`."
                        ))
                        .next(format!("Import `{name}` from `app`")),
                );
            }
            let all = self.table.all();
            let near = nearest(
                name,
                all.declarations
                    .iter()
                    .filter(|(_, found)| found.iter().any(|s| folder_of(s).is_some()))
                    .map(|(name, _)| name.as_str()),
            )
            .map(str::to_string);
            let problem = Diagnostic::new(&rule::IMPORT_UNKNOWN, format!("`{name}` isn't declared"))
                .at(span.clone(), "not declared")
                .reason(format!(
                    "Nothing in this project is called `{name}`, so this line brings in nothing by that name.{}",
                    did_you_mean(near.as_deref())
                ));
            return Some(match near {
                Some(near) => problem
                    .next(format!("Use `{near}` instead of `{name}`"))
                    .edit(span, near),
                None => problem.next(format!("Remove `{name}` from this line")),
            });
        }
        let Some(symbol) = declared.iter().find(|symbol| symbol.library.is_none()) else {
            let library = declared
                .iter()
                .find_map(|symbol| symbol.library.as_ref())
                .map(|(library, _)| library.clone())
                .unwrap_or_default();
            return Some(
                Diagnostic::new(
                    &rule::IMPORT_UNKNOWN,
                    format!("`{name}` is a function in library `{library}`"),
                )
                .at(span.clone(), "a library function")
                .reason(format!(
                    "A library comes in whole, by its name, and its functions are called through it: `{library}.{name}(...)`."
                ))
                .next(format!("Import `{library}` instead"))
                .edit(span, library),
            );
        };
        match folder_of(symbol) {
            Some(home) if home == folder => {
                (!used.contains(name)).then(|| self.unused_name(index, entry, name))
            }
            Some(home) => Some(self.wrong_folder(index, entry, name, folder, &home, symbol)),
            None => Some(
                Diagnostic::new(
                    &rule::IMPORT_UNKNOWN,
                    format!("`{name}` can't be imported"),
                )
                .at(span, "not importable")
                .reason(format!(
                    "Names come in from a domain's folders, `shared/` and the root files; `{name}` is {} in `{}`, which nothing imports from.",
                    symbol.kind.with_a(),
                    symbol.path
                ))
                .next(format!("Remove `{name}` from this line"))
                .edit(self.span_of(&self.remove_entry(index, entry)), ""),
            ),
        }
    }

    fn missing(&self, written: &Written, folder: &str, symbol: Option<&Symbol>) -> Diagnostic {
        let name = &written.name;
        let what = match symbol {
            Some(symbol) => format!("{} in `{}`", symbol.kind.with_a(), symbol.path),
            None => "a connection in `app.jadpo`".to_string(),
        };
        let (at, insert) = self.add(folder, Some(name));
        let line = format!("use {folder} {{ {name} }}");
        let next = if self.line_for(folder).is_some() {
            format!(
                "Add `{name}` to `use {folder} {{ ... }}`, or run `jadpo fmt`, which adds every missing import"
            )
        } else {
            format!("Add `{line}`, or run `jadpo fmt`, which adds every missing import")
        };
        Diagnostic::new(&rule::IMPORT_MISSING, format!("`{name}` isn't imported"))
            .at(self.span(written.start, written.end), "not imported")
            .reason(format!(
                "Every name a file uses that it doesn't declare comes in on a `use` line, so a reader sees where it lives; `{name}` is {what}."
            ))
            .next(next)
            .edit(self.span(at.0, at.1), insert)
    }

    fn missing_domain(&self, written: &Written, domain: &str) -> Diagnostic {
        let (at, insert) = self.add(domain, None);
        Diagnostic::new(&rule::IMPORT_MISSING, format!("`{domain}` isn't imported"))
            .at(self.span(written.start, written.end), "not imported")
            .reason(format!(
                "A call into another domain names it, and the domain comes in on a `use` line of its own: `use {domain}`."
            ))
            .next(format!(
                "Add `use {domain}`, or run `jadpo fmt`, which adds every missing import"
            ))
            .edit(self.span(at.0, at.1), insert)
    }

    fn unused_name(&self, index: usize, entry: usize, name: &str) -> Diagnostic {
        let (_, start, end) = self.entry(index, entry);
        let only = self.entries(index).len() == 1;
        Diagnostic::new(
            &rule::IMPORT_UNUSED,
            format!("`{name}` is imported but not used"),
        )
        .at(self.span(start, end), "not used")
        .reason(format!(
            "A `use` line lists only the names its file uses, so each says where it comes from; nothing here uses `{name}`."
        ))
        .next(if only {
            "Remove this line".to_string()
        } else {
            format!("Remove `{name}` from this line")
        })
        .edit(self.span_of(&self.remove_entry(index, entry)), "")
    }

    fn unused_domain(&self, index: usize, domain: &str) -> Diagnostic {
        let line = &self.lines[index];
        let (start, end) = line.path_span();
        Diagnostic::new(
            &rule::IMPORT_UNUSED,
            format!("`{domain}` is imported but not used"),
        )
        .at(self.span(start, end), "no calls through it")
        .reason(format!(
            "`use {domain}` brings in a domain for calls such as `{domain}.<action>(...)`, and this file makes none."
        ))
        .next("Remove this line")
        .edit(self.span_of(&self.whole_line(index)), "")
    }

    fn own_domain_line(&self, index: usize, domain: &str) -> Diagnostic {
        let line = &self.lines[index];
        let (start, end) = line.path_span();
        Diagnostic::new(
            &rule::IMPORT_UNNEEDED,
            format!("`{domain}` is this file's own domain"),
        )
        .at(self.span(start, end), "own domain")
        .reason(format!(
            "Your own domain's actions and queries are called bare, so `use {domain}` brings in nothing; its names come in by folder, such as `use {domain}/types {{ ... }}`."
        ))
        .next("Remove this line")
        .edit(self.span_of(&self.whole_line(index)), "")
    }

    fn empty_line(&self, index: usize, folder: &str) -> Diagnostic {
        let line = &self.lines[index];
        let (start, end) = line.path_span();
        Diagnostic::new(
            &rule::IMPORT_UNUSED,
            format!("`use {folder}` brings in nothing"),
        )
        .at(self.span(start, end), "no names")
        .reason(format!(
            "The braces list the names the file takes from `{folder}`, and they're empty."
        ))
        .next("Remove this line")
        .edit(self.span_of(&self.whole_line(index)), "")
    }

    fn unneeded(&self, index: usize, entry: usize, message: String, reason: String) -> Diagnostic {
        let (name, start, end) = self.entry(index, entry);
        let only = self.entries(index).len() == 1;
        Diagnostic::new(&rule::IMPORT_UNNEEDED, message)
            .at(self.span(start, end), "needs no import")
            .reason(reason)
            .next(if only {
                "Remove this line".to_string()
            } else {
                format!("Remove `{name}` from this line")
            })
            .edit(self.span_of(&self.remove_entry(index, entry)), "")
    }

    fn duplicate_name(
        &self,
        index: usize,
        entry: usize,
        name: &str,
        first: (u32, u32),
    ) -> Diagnostic {
        let (_, start, end) = self.entry(index, entry);
        Diagnostic::new(
            &rule::IMPORT_DUPLICATE,
            format!("`{name}` is imported twice"),
        )
        .at(self.span(start, end), "imported again")
        .also(self.span(first.0, first.1), "first imported here")
        .reason("Each name is imported once, on the line of the folder that holds it.")
        .next(format!("Remove this `{name}`"))
        .edit(self.span_of(&self.remove_entry(index, entry)), "")
    }

    fn duplicate_line(&self, index: usize, first: usize) -> Diagnostic {
        let line = &self.lines[index];
        let folder = line.folder();
        let (start, end) = line.path_span();
        let (first_start, first_end) = self.lines[first].path_span();
        let problem = Diagnostic::new(
            &rule::IMPORT_DUPLICATE,
            format!("`{folder}` has two `use` lines"),
        )
        .at(self.span(start, end), "a second line")
        .also(self.span(first_start, first_end), "first line")
        .reason(format!(
            "Each folder has one `use` line, listing every name the file takes from it; `use {folder}` is written twice."
        ));
        let moved: Vec<&str> = self
            .entries(index)
            .iter()
            .map(|(name, _, _)| name.as_str())
            .filter(|name| !self.entries(first).iter().any(|(n, _, _)| n == name))
            .collect();
        let removal = self.span_of(&self.whole_line(index));
        if moved.is_empty() {
            return problem.next("Remove the second line").edit(removal, "");
        }
        let (at, insert) = self.insert_names(first, &moved);
        problem
            .next(format!("Move its names onto the first `{folder}` line"))
            .edit(self.span(at.0, at.1), insert)
            .edit(removal, "")
    }

    fn wrong_folder(
        &self,
        index: usize,
        entry: usize,
        name: &str,
        folder: &str,
        home: &str,
        symbol: &Symbol,
    ) -> Diagnostic {
        let (_, start, end) = self.entry(index, entry);
        let problem = Diagnostic::new(
            &rule::IMPORT_UNKNOWN,
            format!("`{name}` isn't in `{folder}`"),
        )
        .at(self.span(start, end), "not in this folder")
        .reason(format!(
            "A `use` line brings in names from the folder it names; `{name}` is {} in `{}`, so it comes in from `{home}`.",
            symbol.kind.with_a(),
            symbol.path
        ))
        .next(format!("Import `{name}` from `{home}`"));
        let only = self.entries(index).len() == 1;
        match self.line_for(home) {
            // Another line already names its folder: move it there.
            Some(target) => {
                let (at, insert) = self.insert_names(target, &[name]);
                problem
                    .edit(self.span(at.0, at.1), insert)
                    .edit(self.span_of(&self.remove_entry(index, entry)), "")
            }
            // It is this line's only name: the line names the right folder.
            None if only => {
                let (s, e) = self.lines[index].path_span();
                problem.edit(self.span(s, e), home)
            }
            None => {
                let after = self.line_end_before_newline(index);
                problem
                    .edit(self.span_of(&self.remove_entry(index, entry)), "")
                    .edit(
                        self.span(after, after),
                        format!("\nuse {home} {{ {name} }}"),
                    )
            }
        }
    }

    // --- edits ---------------------------------------------------------

    fn span_of(&self, range: &(u32, u32)) -> Span {
        self.span(range.0, range.1)
    }

    fn entries(&self, index: usize) -> &[(String, u32, u32)] {
        self.lines[index]
            .names
            .as_ref()
            .map_or(&[][..], |names| &names.entries[..])
    }

    fn entry(&self, index: usize, entry: usize) -> (String, u32, u32) {
        self.entries(index)[entry].clone()
    }

    /// The first line naming `folder` with braces.
    fn line_for(&self, folder: &str) -> Option<usize> {
        self.lines
            .iter()
            .position(|line| line.folder() == folder && line.names.is_some())
    }

    /// The range to remove for name `entry` of line `index`: the name and
    /// the comma beside it, or the whole line when it is the only name.
    fn remove_entry(&self, index: usize, entry: usize) -> (u32, u32) {
        let entries = self.entries(index);
        if entries.len() <= 1 {
            return self.whole_line(index);
        }
        if entry + 1 < entries.len() {
            (entries[entry].1, entries[entry + 1].1)
        } else {
            (entries[entry - 1].2, entries[entry].2)
        }
    }

    /// A whole line, from the start of its line to the start of the next.
    fn whole_line(&self, index: usize) -> (u32, u32) {
        whole_line(self.text, &self.lines[index])
    }

    /// The end of a line's last token: where a line inserted after it
    /// starts, with `\n`.
    fn line_end_before_newline(&self, index: usize) -> u32 {
        self.lines[index].end
    }

    /// The edit that adds `names` to line `index`'s braces.
    fn insert_names(&self, index: usize, names: &[&str]) -> ((u32, u32), String) {
        let line = &self.lines[index];
        let list = names.join(", ");
        match &line.names {
            Some(braces) => match braces.entries.last() {
                Some((_, _, end)) => ((*end, *end), format!(", {list}")),
                None => {
                    let end = braces.close.map_or(braces.open.1, |close| close.1);
                    ((braces.open.0, end), format!("{{ {list} }}"))
                }
            },
            None => ((line.end, line.end), format!(" {{ {list} }}")),
        }
    }

    /// The edit that brings `name` in from `folder`, or the domain
    /// `folder` when `name` is `None`: onto the folder's line when the file
    /// has one, otherwise on a new line after the last `use` line, or at
    /// the top of the file.
    fn add(&self, folder: &str, name: Option<&str>) -> ((u32, u32), String) {
        if let Some(name) = name
            && let Some(index) = self.line_for(folder)
        {
            return self.insert_names(index, &[name]);
        }
        let line = match name {
            Some(name) => format!("use {folder} {{ {name} }}"),
            None => format!("use {folder}"),
        };
        match self.lines.last() {
            Some(last) => ((last.end, last.end), format!("\n{line}")),
            None => ((self.top, self.top), format!("{line}\n\n")),
        }
    }
}

/// The edits that move declaration `decl` of `file`, called `name`, to a
/// new file at `target` (area 1, "Sharing and promotion"): taken out of
/// its file with the comments attached to it, written into the new file
/// below the `use` lines it needs, and imported back into its old file
/// when that still uses it. Files that import it from its old folder are
/// `jadpo fmt`'s to update, as for any name imported from the wrong
/// folder.
pub(crate) fn move_edits(
    db: &dyn Db,
    project: Project,
    file: File,
    decl: &SyntaxNode,
    name: &str,
    target: &str,
) -> Vec<(Span, String)> {
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let path = file.path(db);
    let text = file.text(db);
    let tokens: Vec<SyntaxToken> = decl
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    let (Some(first), Some(last)) = (tokens.first(), tokens.last()) else {
        return Vec::new();
    };
    let start = attached_start(first);
    let end = u32::from(last.text_range().end());
    let moved = &text[start as usize..end as usize];
    // Up to whatever follows, a comment of its own included, or, for the
    // last declaration, back to the end of the line before it. A comment
    // after its last token on the same line goes with it.
    let mut after = None;
    let mut same_line = true;
    let mut current = last.next_token();
    while let Some(token) = current {
        match token.kind() {
            WHITESPACE => same_line &= !token.text().contains('\n'),
            COMMENT | DOC_COMMENT if same_line => {}
            _ => {
                after = Some(u32::from(token.text_range().start()));
                break;
            }
        }
        current = token.next_token();
    }
    let removal = match after {
        Some(next) => (start, next),
        None => {
            let before = first
                .prev_token()
                .into_iter()
                .flat_map(|token| std::iter::successors(Some(token), SyntaxToken::prev_token))
                .find(|token| !token.kind().is_trivia());
            match before {
                Some(before) => {
                    let after = u32::from(before.text_range().end()) as usize;
                    let line_end = text[after..]
                        .find('\n')
                        .map_or(text.len(), |i| after + i + 1);
                    ((line_end as u32).min(start), text.len() as u32)
                }
                None => (start, text.len() as u32),
            }
        }
    };
    let lookup = Lookup::new(db, project);
    let new_location = jadpo_db::locate(target, Layout::Project);
    let new_here = Here {
        path: target,
        text: "",
        location: &new_location,
        subject: None,
        table: &lookup,
        std: StdNames::get(),
        lines: Vec::new(),
        top: 0,
    };
    let mut wanted: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut domains: BTreeSet<String> = BTreeSet::new();
    for written in written(decl) {
        match new_here.need(&written) {
            Some(Need::Name(symbol)) if symbol.name != name && new_here.may_use(&symbol) => {
                if let Some(folder) = folder_of(&symbol) {
                    wanted
                        .entry(folder)
                        .or_default()
                        .insert(symbol.name.clone());
                }
            }
            Some(Need::Connection(connection)) => {
                wanted
                    .entry("app".to_string())
                    .or_default()
                    .insert(connection);
            }
            Some(Need::Domain(domain)) if new_here.own_domain() != Some(domain.as_str()) => {
                domains.insert(domain);
            }
            _ => {}
        }
    }
    let mut lines: Vec<String> = wanted
        .iter()
        .map(|(folder, names)| {
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            format!("use {folder} {{ {} }}", names.join(", "))
        })
        .collect();
    lines.extend(domains.iter().map(|domain| format!("use {domain}")));
    let mut created = String::new();
    if !lines.is_empty() {
        created.push_str(&lines.join("\n"));
        created.push_str("\n\n");
    }
    created.push_str(moved);
    created.push('\n');
    let created = jadpo_fmt::format_file(&created).unwrap_or(created);
    let mut edits = vec![(Span::new(target, 0, 0), created)];
    // The old file keeps using it: import it from where it now lives.
    let still_used = source.syntax().descendants().any(|node| {
        node.kind() == NAME_REF && !node.ancestors().any(|a| a.kind() == USE_DECL) && {
            let range = node.text_range();
            let (s, e) = (u32::from(range.start()), u32::from(range.end()));
            (e <= start || s >= end) && named(&node).is_some_and(|(n, _, _)| n == name)
        }
    });
    let folder = match (&new_location.ownership, &new_location.place) {
        (Ownership::Domain(domain), Place::Folder(inner)) => {
            Some(format!("{domain}/{}", inner.name()))
        }
        (Ownership::Shared, Place::Folder(inner)) => Some(format!("shared/{}", inner.name())),
        _ => None,
    };
    let old_location = location(db, project, file);
    let old_here = Here {
        path,
        text,
        location: old_location,
        subject: None,
        table: &lookup,
        std: StdNames::get(),
        lines: use_lines(source.syntax()),
        top: top(source.syntax()),
    };
    if still_used && let Some(folder) = folder {
        let ((at, until), insert) = old_here.add(&folder, Some(name));
        if at == removal.0 && at == until {
            edits.push((Span::new(path, removal.0, removal.1), insert));
            return edits;
        }
        edits.push((Span::new(path, at, until), insert));
    }
    edits.push((Span::new(path, removal.0, removal.1), String::new()));
    edits
}

/// Where the comments attached to `token` start: those directly above
/// it, with no blank line between.
fn attached_start(token: &SyntaxToken) -> u32 {
    let mut at = u32::from(token.text_range().start());
    let mut current = token.prev_token();
    while let Some(trivia) = current {
        match trivia.kind() {
            WHITESPACE if trivia.text().matches('\n').count() >= 2 => break,
            WHITESPACE => {}
            COMMENT | DOC_COMMENT => {
                // A comment trailing the previous token's line is that
                // token's.
                let own_line = trivia.prev_token().is_none_or(|before| {
                    before.kind() == WHITESPACE && before.text().contains('\n')
                });
                if !own_line {
                    break;
                }
                at = u32::from(trivia.text_range().start());
            }
            _ => break,
        }
        current = trivia.prev_token();
    }
    at
}

#[cfg(test)]
mod tests {
    use jadpo_db::{Folder, RootFile};

    #[test]
    fn the_formatter_sorts_by_the_layouts_folders_and_root_files() {
        let folders: Vec<&str> = Folder::ALL.iter().map(|folder| folder.name()).collect();
        assert_eq!(folders, jadpo_fmt::FOLDERS);
        let roots: Vec<&str> = RootFile::ALL
            .iter()
            .map(|root| root.file_name().trim_end_matches(".jadpo"))
            .collect();
        assert_eq!(roots, jadpo_fmt::ROOT_FILES);
    }
}
