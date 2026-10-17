//! Fixing a finding the standard types raise (area 19, question 4; area
//! 14's `jadpo fix`; chunk 5.5a2): `jadpo fix` and the editor's quick fix
//! both run this code, so they fix the same way and refuse the same
//! things.
//!
//! Two findings have fixes. `type.standard_clash` is a declaration named
//! like a standard type, which a release added after the application named
//! its own: it is swapped for the standard type (a type only) or renamed.
//! `type.standard_exists` is a type with a standard type's shape under
//! another name: it is swapped.
//!
//! - **The swap** makes every reference to the application's type a
//!   reference to the standard one, each field the shape matched read by
//!   the standard field's name (`starts_at` becomes `start`, in a body by
//!   what each name resolved to, elsewhere as `Type.field` is written), and
//!   deletes the declaration, and its file when nothing is left in it.
//! - **The rename** writes the new name at the declaration and at every
//!   reference in the files that can see it, and moves the file named after
//!   the declaration to the new name's file. It is not `jadpo rename`
//!   (area 14, after the first build): it writes no identity line, so a
//!   rename whose result changes stored data or an API record stops, as
//!   every fix that adds an error does.
//!
//! As in `setting_move`, [`plan`] reads and decides, editing nothing, and
//! [`apply`] edits the loaded project's texts, fixes the imports of each
//! file it touched, formats each file that was formatted and checks the
//! result. A fix never adds an error: one whose result has an error the
//! project didn't have before (by rule and headline) is undone and
//! refused with those errors. Errors the project already had stay, and
//! the command says how many are left. The command writes what [`apply`]
//! gives back; the editor sends it as one workspace edit and puts the
//! server's texts back with [`restore`].

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, File, ItemKind, Loaded, Ownership, Project, Symbol};
use jadpo_diag::{Diagnostic, Rule, Span, nearest, rule};
use jadpo_graph::Kind;
use jadpo_graph::body::Refers;
use jadpo_syntax::SyntaxKind::*;
use jadpo_syntax::ast::{self, AstNode};

use crate::case::{is_upper_camel, round_trips, to_snake};
use crate::setting_move::{FileChange, Mark, Row, Stopped};
use crate::std_lib::StdNames;
use crate::tree;
use crate::types::{callables_of, type_table, typed_body};

/// The two fixes, as `jadpo fix` writes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FixKind {
    Swap,
    Rename,
}

impl FixKind {
    pub const ALL: [FixKind; 2] = [FixKind::Swap, FixKind::Rename];

    pub fn word(self) -> &'static str {
        match self {
            FixKind::Swap => "swap",
            FixKind::Rename => "rename",
        }
    }

    pub fn from_word(word: &str) -> Option<FixKind> {
        FixKind::ALL.into_iter().find(|fix| fix.word() == word)
    }
}

/// The fix chosen, with what it needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fix {
    Swap,
    Rename(String),
}

impl Fix {
    pub fn kind(&self) -> FixKind {
        match self {
            Fix::Swap => FixKind::Swap,
            Fix::Rename(_) => FixKind::Rename,
        }
    }
}

/// One finding a fix answers.
#[derive(Clone, Debug)]
pub struct Finding {
    /// The declaration's name, as written.
    pub name: String,
    pub rule: &'static Rule,
    pub path: String,
    /// The declaration's line, from 1.
    pub line: u32,
    /// The declaration's name in its file.
    pub span: Span,
    /// Its position among the file's declarations.
    pub index: u32,
    /// The standard type it clashes with or copies.
    pub standard: &'static str,
    /// The release that added the standard type.
    pub release: &'static str,
    /// The fixes it takes, swap first.
    pub fixes: Vec<FixKind>,
    /// The declaration's kind in the reader's words: "type", "entity".
    pub word: &'static str,
}

/// The findings of `project` a fix answers, in file and source order.
pub fn findings(db: &dyn Db, project: Project) -> Vec<Finding> {
    crate::check(db, project)
        .iter()
        .filter_map(|problem| finding_of(db, project, problem))
        .collect()
}

/// The finding `problem` is, when a fix answers it.
pub fn finding_of(db: &dyn Db, project: Project, problem: &Diagnostic) -> Option<Finding> {
    let clash = problem.rule.id == rule::TYPE_STANDARD_CLASH.id;
    if !clash && problem.rule.id != rule::TYPE_STANDARD_EXISTS.id {
        return None;
    }
    let label = problem.primary.as_ref()?;
    let file = file_at(db, project, &label.span.path)?;
    let text = file.text(db);
    let name = text
        .get(label.span.start as usize..label.span.end as usize)?
        .to_string();
    let source = tree::source(db, file)?;
    let item = jadpo_db::item_tree(db, file)
        .items
        .iter()
        .find(|item| {
            item.name == name
                && tree::name_span(&source, &label.span.path, item.index, None) == label.span
        })?
        .clone();
    let line = text[..label.span.start as usize].matches('\n').count() as u32 + 1;
    let (standard, fixes) = if clash {
        let standard = jadpo_std::standard_type(&name)?;
        let fixes = if is_type(&item.kind) {
            vec![FixKind::Swap, FixKind::Rename]
        } else {
            vec![FixKind::Rename]
        };
        (standard, fixes)
    } else {
        let table = type_table(db, project);
        let found = crate::types::decl::standard_match(table, &name, None)?;
        (found.standard, vec![FixKind::Swap])
    };
    Some(Finding {
        name,
        rule: problem.rule,
        path: label.span.path.clone(),
        line,
        span: label.span.clone(),
        index: item.index,
        standard: standard.name,
        release: standard.release,
        fixes,
        word: item.kind.word(),
    })
}

/// Whether a declaration of this kind is a type the standard one can
/// replace: not a failure, an event, an email or an entity.
fn is_type(kind: &ItemKind) -> bool {
    matches!(kind, ItemKind::Type(_)) && kind.word() == "type"
}

fn file_at(db: &dyn Db, project: Project, path: &str) -> Option<File> {
    project
        .files(db)
        .iter()
        .copied()
        .find(|file| file.path(db) == path)
}

/// The finding `written` names on the command line, by its declaration's
/// name.
pub fn find(db: &dyn Db, project: Project, written: &str) -> Result<Finding, Box<Diagnostic>> {
    let all = findings(db, project);
    let named: Vec<&Finding> = all.iter().filter(|f| f.name == written).collect();
    match named.as_slice() {
        [finding] => Ok((*finding).clone()),
        [] => {
            let diagnostic = Diagnostic::new(
                &rule::CLI_FIX_ARGUMENTS,
                format!("`{written}` has nothing to fix"),
            );
            let names: Vec<&str> = all.iter().map(|f| f.name.as_str()).collect();
            Err(Box::new(match nearest(written, names.iter().copied()) {
                Some(near) => diagnostic
                    .reason(format!(
                        "`jadpo fix` fixes a declaration that `type.standard_clash` or `type.standard_exists` names, and neither names `{written}`. Did you mean `{near}`?"
                    ))
                    .next(format!("Use `jadpo fix {near}`")),
                None if all.is_empty() => diagnostic
                    .reason("`jadpo fix` fixes a declaration that `type.standard_clash` or `type.standard_exists` names, and the project has neither finding.")
                    .next("Run `jadpo check` to see the project's errors"),
                None => diagnostic
                    .reason(format!(
                        "`jadpo fix` fixes a declaration that `type.standard_clash` or `type.standard_exists` names, and neither names `{written}`."
                    ))
                    .next("Run `jadpo fix` to list what it can fix"),
            }))
        }
        many => {
            let places: Vec<String> = many
                .iter()
                .map(|f| format!("`{}:{}`", f.path, f.line))
                .collect();
            Err(Box::new(
                Diagnostic::new(
                    &rule::CLI_FIX_ARGUMENTS,
                    format!("`{written}` is declared in more than one place"),
                )
                .reason(format!(
                    "Domains may each declare a private type of one name, and `{written}` is declared at {}; a fix changes one declaration and the code that sees it.",
                    crate::and_list(&places)
                ))
                .next(format!(
                    "Rename all but one `{written}` by hand, then run `jadpo fix {written}` again"
                )),
            ))
        }
    }
}

/// The finding whose declaration's name is at byte `offset` of `file`:
/// where the editor offers the swap.
pub fn at(db: &dyn Db, project: Project, file: File, offset: u32) -> Vec<Finding> {
    let path = file.path(db);
    findings(db, project)
        .into_iter()
        .filter(|f| f.path == *path && f.span.start <= offset && offset <= f.span.end)
        .collect()
}

// --- planning -------------------------------------------------------------------

/// One text replacement, with its line in the text before.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Edit {
    start: u32,
    end: u32,
    text: String,
    line: u32,
    /// What it changes, for the change list: the name written, and the
    /// name it was.
    was: String,
    /// Whether the change list shows it; imports are left to the diff.
    shown: bool,
}

/// What [`plan`] decided.
#[derive(Clone, Debug)]
pub struct Plan {
    pub finding: Finding,
    pub fix: Fix,
    /// Each file's replacements, in path order.
    edits: BTreeMap<String, Vec<Edit>>,
    /// The file a rename moves the declaration's file to.
    moves_to: Option<String>,
}

/// Decides how `fix` answers `finding`, editing nothing; a fix the finding
/// doesn't take, or a new name that can't be, is refused.
pub fn plan(
    db: &dyn Db,
    project: Project,
    finding: &Finding,
    fix: &Fix,
) -> Result<Plan, Vec<Diagnostic>> {
    if !finding.fixes.contains(&fix.kind()) {
        let reason = match fix.kind() {
            FixKind::Swap => format!(
                "Only a type can be swapped for a standard type, and `{}` is {} {}.",
                finding.name,
                a_or_an(finding.word),
                finding.word
            ),
            FixKind::Rename => format!(
                "`{}` has the standard `{}`'s shape under another name, so a new name would still have it; swap it, or decline the finding with a reason.",
                finding.name, finding.standard
            ),
        };
        let offered: Vec<String> = finding
            .fixes
            .iter()
            .map(|fix| format!("`jadpo fix {} {}`", finding.name, fix.word()))
            .collect();
        return Err(vec![
            Diagnostic::new(
                &rule::CLI_FIX_ARGUMENTS,
                format!(
                    "`{}` can't be fixed with `{}`",
                    finding.name,
                    fix.kind().word()
                ),
            )
            .reason(reason)
            .next(format!("Run {}", crate::or_list(&offered))),
        ]);
    }
    let symbols = jadpo_db::symbols(db, project);
    let Some(symbol) = symbols
        .declarations
        .get(&finding.name)
        .and_then(|all| {
            all.iter()
                .find(|s| s.path == finding.path && s.index == finding.index)
        })
        .cloned()
    else {
        return Err(Vec::new());
    };
    let mut edits: BTreeMap<String, Vec<Edit>> = BTreeMap::new();
    let mut moves_to = None;
    match fix {
        Fix::Swap => {
            let fields = swapped_fields(db, project, finding);
            references(
                db,
                project,
                &symbol,
                finding,
                finding.standard,
                &fields,
                false,
                &mut edits,
            );
        }
        Fix::Rename(new) => {
            if let Some(problem) = refused_name(db, project, finding, new) {
                return Err(vec![problem]);
            }
            references(
                db,
                project,
                &symbol,
                finding,
                new,
                &BTreeMap::new(),
                true,
                &mut edits,
            );
            edits.entry(finding.path.clone()).or_default().push(Edit {
                start: finding.span.start,
                end: finding.span.end,
                text: new.clone(),
                line: finding.line,
                was: finding.name.clone(),
                shown: true,
            });
            let location = jadpo_db::locate(&finding.path, project.layout(db));
            if project.layout(db) == jadpo_db::Layout::Project
                && !location.test
                && location.stem == to_snake(&finding.name)
            {
                let folder = finding
                    .path
                    .rsplit_once('/')
                    .map_or(String::new(), |(folder, _)| format!("{folder}/"));
                let to = format!("{folder}{}.jadpo", to_snake(new));
                if file_at(db, project, &to).is_some() {
                    return Err(vec![
                        Diagnostic::new(
                            &rule::CLI_FIX_ARGUMENTS,
                            format!("`{to}` already exists"),
                        )
                        .reason(format!(
                            "A declaration lives in the file named after it, so renaming `{}` moves `{}` to `{to}`, which is taken.",
                            finding.name, finding.path
                        ))
                        .next("Choose another name, or move that file first"),
                    ]);
                }
                moves_to = Some(to);
            }
        }
    }
    for list in edits.values_mut() {
        list.sort_by_key(|edit| (edit.start, edit.end));
        list.dedup_by_key(|edit| (edit.start, edit.end));
    }
    Ok(Plan {
        finding: finding.clone(),
        fix: fix.clone(),
        edits,
        moves_to,
    })
}

fn a_or_an(word: &str) -> &'static str {
    if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

/// Each field of the finding's type that the standard type names
/// differently, by its name now: `starts_at` to `start`. A clash's fields
/// are matched against the standard type of its own name only.
fn swapped_fields(
    db: &dyn Db,
    project: Project,
    finding: &Finding,
) -> BTreeMap<String, &'static str> {
    let table = type_table(db, project);
    let only = (finding.standard == finding.name).then_some(finding.standard);
    crate::types::decl::standard_match(table, &finding.name, only)
        .map(|found| {
            found
                .fields
                .into_iter()
                .filter(|field| field.written != field.standard)
                .map(|field| (field.written, field.standard))
                .collect()
        })
        .unwrap_or_default()
}

/// Why `new` can't be the finding's new name, if it can't.
fn refused_name(db: &dyn Db, project: Project, finding: &Finding, new: &str) -> Option<Diagnostic> {
    let problem = |headline: String, reason: String| {
        Diagnostic::new(&rule::CLI_FIX_ARGUMENTS, headline)
            .reason(reason)
            .next(format!(
                "Run `jadpo fix {} rename <NewName>` with another name",
                finding.name
            ))
    };
    if !is_upper_camel(new) || !round_trips(new) {
        return Some(problem(
            format!("`{new}` isn't a name for a {}", finding.word),
            format!(
                "A {}'s name is written in UpperCamelCase and reads back from its file name unchanged, as `RoomReference` does from `room_reference.jadpo`.",
                finding.word
            ),
        ));
    }
    if new == finding.name {
        return Some(problem(
            format!("`{new}` is the name it has"),
            format!(
                "Renaming `{new}` to itself would leave it clashing with the standard `{new}`."
            ),
        ));
    }
    if jadpo_std::standard_type(new).is_some() || StdNames::get().is_type(new) {
        return Some(problem(
            format!("`{new}` is a built-in type"),
            "Built-in and standard types exist without being declared, and their names can't be reused.".to_string(),
        ));
    }
    if StdNames::get().reserved(new) {
        return Some(problem(
            format!("`{new}` is a standard-library namespace"),
            "Standard-library namespaces can't name a declaration.".to_string(),
        ));
    }
    if let Some(taken) = jadpo_db::symbols(db, project)
        .declarations
        .get(new)
        .and_then(|all| all.first())
    {
        return Some(problem(
            format!("`{new}` is already declared"),
            format!(
                "`{new}` is declared in `{}`, and names are unique, so it can't be `{}`'s new name too.",
                taken.path, finding.name
            ),
        ));
    }
    None
}

/// Every reference to the finding's declaration in the files that can see
/// it, written as `to`, and each field `fields` renames where it is read;
/// `use` lines are renamed only for a rename, since a swap's standard type
/// is never imported. The declaration itself is left out: a swap deletes
/// it, and a rename writes its name separately; a swap leaves out what is
/// inside the declaration, which goes.
#[allow(clippy::too_many_arguments)]
fn references(
    db: &dyn Db,
    project: Project,
    symbol: &Symbol,
    finding: &Finding,
    to: &str,
    fields: &BTreeMap<String, &'static str>,
    rename: bool,
    edits: &mut BTreeMap<String, Vec<Edit>>,
) {
    let name = finding.name.as_str();
    for &file in project.files(db) {
        let ownership = jadpo_db::location(db, project, file).ownership.clone();
        if !sees(symbol, &ownership) {
            continue;
        }
        let path = file.path(db).clone();
        let text = file.text(db);
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        let line_of = |at: u32| text[..at as usize].matches('\n').count() as u32 + 1;
        let skip = (path == finding.path && !rename)
            .then(|| tree::decl_at(&source, finding.index))
            .flatten()
            .map(|decl| decl.syntax().text_range());
        let mut out: Vec<Edit> = Vec::new();
        for name_ref in source.syntax().descendants().filter_map(ast::NameRef::cast) {
            let Some(token) = name_ref.ident_token() else {
                continue;
            };
            if token.text() != name {
                continue;
            }
            let in_use = name_ref
                .syntax()
                .ancestors()
                .any(|node| ast::UseDecl::can_cast(node.kind()));
            if in_use && !rename {
                continue;
            }
            let range = token.text_range();
            let (start, end) = (u32::from(range.start()), u32::from(range.end()));
            if to != name {
                out.push(Edit {
                    start,
                    end,
                    text: to.to_string(),
                    line: line_of(start),
                    was: name.to_string(),
                    shown: true,
                });
            }
            // `Type.field`, written outside a body or inside one.
            if !fields.is_empty() {
                let mut next = token.next_token();
                while let Some(t) = next.as_ref().filter(|t| t.kind().is_trivia()) {
                    next = t.next_token();
                }
                if next.as_ref().is_some_and(|t| t.kind() == DOT) {
                    let mut field = next.and_then(|t| t.next_token());
                    while let Some(t) = field.as_ref().filter(|t| t.kind().is_trivia()) {
                        field = t.next_token();
                    }
                    if let Some(field) = field.filter(|t| t.kind() == IDENT)
                        && let Some(standard) = fields.get(field.text())
                    {
                        let range = field.text_range();
                        let start = u32::from(range.start());
                        out.push(Edit {
                            start,
                            end: u32::from(range.end()),
                            text: (*standard).to_string(),
                            line: line_of(start),
                            was: field.text().to_string(),
                            shown: true,
                        });
                    }
                }
            }
        }
        // Each field read in a body, by what its name resolved to.
        if !fields.is_empty() {
            for callable in callables_of(db, file) {
                let body = &typed_body(db, project, callable).body;
                for written in &body.names {
                    let Refers::Node { key, .. } = &written.refers else {
                        continue;
                    };
                    if key.kind() != Some(Kind::Field) {
                        continue;
                    }
                    let Some((host, field)) = key.written().split_once('.') else {
                        continue;
                    };
                    if host != name || field.contains('.') {
                        continue;
                    }
                    if let Some(standard) = fields.get(field) {
                        out.push(Edit {
                            start: written.start,
                            end: written.end,
                            text: (*standard).to_string(),
                            line: line_of(written.start),
                            was: field.to_string(),
                            shown: true,
                        });
                    }
                }
            }
        }
        // A swap's standard type is never imported, so `use` lines lose the
        // name, and a line that named nothing else goes.
        if !rename {
            for line in source.use_decls() {
                let Some(names) = line.use_names() else {
                    continue;
                };
                let written: Vec<String> = names
                    .name_refs()
                    .filter_map(|name| tree::name_ref_text(&name))
                    .collect();
                if !written.iter().any(|n| n == name) {
                    continue;
                }
                let rest: Vec<&str> = written
                    .iter()
                    .map(String::as_str)
                    .filter(|n| *n != name)
                    .collect();
                let (start, end, replacement) = if rest.is_empty() {
                    let range = line.syntax().text_range();
                    let start = text[..usize::from(range.start())]
                        .rfind('\n')
                        .map_or(0, |i| i + 1);
                    let tail = &text[usize::from(range.end())..];
                    let end =
                        usize::from(range.end()) + tail.find('\n').map_or(tail.len(), |i| i + 1);
                    (start as u32, end as u32, String::new())
                } else {
                    let range = names.syntax().text_range();
                    (
                        u32::from(range.start()),
                        u32::from(range.end()),
                        format!("{{ {} }}", rest.join(", ")),
                    )
                };
                out.retain(|edit| !(start <= edit.start && edit.end <= end));
                out.push(Edit {
                    start,
                    end,
                    text: replacement,
                    line: line_of(start),
                    was: name.to_string(),
                    shown: false,
                });
            }
        }
        if let Some(range) = skip {
            out.retain(|edit| {
                !(u32::from(range.start()) <= edit.start && edit.end <= u32::from(range.end()))
            });
        }
        if !out.is_empty() {
            edits.entry(path).or_default().extend(out);
        }
    }
}

/// Whether code owned by `from` can name the declaration: anywhere for a
/// shared, root or loose one, and in its own domain, or any for a public
/// one.
fn sees(symbol: &Symbol, from: &Ownership) -> bool {
    symbol.visible_from(from)
}

// --- applying -------------------------------------------------------------------

/// What [`apply`] did: every file it changed, the declaration's first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fixed {
    pub files: Vec<FileChange>,
    /// The errors the project still has, every one of which it had
    /// before.
    pub errors_left: usize,
}

/// Makes the plan's edits in `loaded`, then fixes imports, formats and
/// checks; undone, with the errors it would add, when the result has an
/// error the project didn't have.
pub fn apply(loaded: &mut Loaded, plan: &Plan) -> Result<Fixed, Stopped> {
    let project = loaded.project;
    let finding = &plan.finding;
    let before_errors = headlines(&crate::check(&loaded.db, project), Some(finding));
    let text_of =
        |loaded: &Loaded, path: &str| loaded.file(path).map(|file| file.text(&loaded.db).clone());
    let mut paths: BTreeSet<String> = plan.edits.keys().cloned().collect();
    paths.insert(finding.path.clone());
    if let Some(to) = &plan.moves_to {
        paths.insert(to.clone());
    }
    let mut before: BTreeMap<String, Option<String>> = BTreeMap::new();
    for path in &paths {
        before.insert(path.clone(), text_of(loaded, path));
    }

    // 1. The references.
    for (path, edits) in &plan.edits {
        let text = text_of(loaded, path).unwrap_or_default();
        let replacements = edits
            .iter()
            .map(|edit| (edit.start, edit.end, edit.text.clone()))
            .collect();
        loaded.set_text(path, crate::apply_edits(&text, replacements));
    }

    // 2. A swap deletes the declaration, and its file when it is all the
    //    file holds; a rename moves the file named after it.
    let mut file_removed = false;
    match &plan.fix {
        Fix::Swap => {
            if let Some(text) = text_of(loaded, &finding.path) {
                let parse = jadpo_syntax::parse_file(&text);
                if let Some(source) = ast::SourceFile::cast(parse.syntax())
                    && let Some(decl) = tree::decl_at(&source, finding.index)
                {
                    if source.decls().count() == 1 {
                        file_removed = true;
                        loaded.remove(&finding.path);
                    } else {
                        loaded.set_text(
                            &finding.path,
                            crate::setting_move::remove_node(&text, decl.syntax()),
                        );
                    }
                }
            }
        }
        Fix::Rename(_) => {
            if let Some(to) = &plan.moves_to
                && let Some(text) = text_of(loaded, &finding.path)
            {
                loaded.remove(&finding.path);
                loaded.set_text(to, text);
            }
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
    let layout_project = project.layout(&loaded.db) == jadpo_db::Layout::Project;
    for path in &present {
        let Some(after) = text_of(loaded, path) else {
            continue;
        };
        let was = match (&plan.moves_to, before.get(path).cloned().flatten()) {
            (Some(to), _) if to == path => before.get(&finding.path).cloned().flatten(),
            (_, was) => was,
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

    // 4. Checked: a fix never adds an error.
    let problems = crate::check(&loaded.db, project);
    let mut had = before_errors;
    let mut added: Vec<Diagnostic> = Vec::new();
    for problem in &problems {
        let key = (problem.rule.id.to_string(), problem.message.clone());
        match had.get_mut(&key) {
            Some(count) if *count > 0 => *count -= 1,
            _ => added.push(problem.clone()),
        }
    }
    let after: BTreeMap<String, Option<String>> = paths
        .iter()
        .map(|path| (path.clone(), text_of(loaded, path)))
        .collect();
    if !added.is_empty() {
        let sources = jadpo_db::source_texts(&loaded.db, project);
        crate::setting_move::restore_texts(loaded, &before);
        return Err(Stopped {
            problems: added,
            sources,
        });
    }

    // What changed, file by file: the declaration's file first.
    let mut order: Vec<String> = vec![finding.path.clone()];
    if let Some(to) = &plan.moves_to {
        order.push(to.clone());
    }
    order.extend(
        paths
            .iter()
            .filter(|p| !order.contains(p))
            .cloned()
            .collect::<Vec<_>>(),
    );
    let file_name = |path: &str| path.rsplit('/').next().unwrap_or(path).to_string();
    let mut files = Vec::new();
    for path in order {
        let was = before.get(&path).cloned().flatten();
        let now = after.get(&path).cloned().flatten();
        if was == now {
            continue;
        }
        let mut rows = Vec::new();
        if path == finding.path {
            match (&plan.fix, &plan.moves_to) {
                (Fix::Swap, _) => {
                    rows.push(Row {
                        mark: Mark::Removed,
                        subject: finding.name.clone(),
                        note: format!(
                            "{}, the standard {} replaces it",
                            finding.word, finding.standard
                        ),
                    });
                    if file_removed {
                        rows.push(Row {
                            mark: Mark::Removed,
                            subject: file_name(&path),
                            note: "file, nothing left in it".to_string(),
                        });
                    }
                }
                (Fix::Rename(_), Some(to)) => rows.push(Row {
                    mark: Mark::Removed,
                    subject: file_name(&path),
                    note: format!("file, renamed {}", file_name(to)),
                }),
                (Fix::Rename(_), None) => {}
            }
        }
        if plan.moves_to.as_deref() == Some(path.as_str()) {
            rows.push(Row {
                mark: Mark::Added,
                subject: file_name(&path),
                note: format!("file, was {}", file_name(&finding.path)),
            });
        }
        let edited = if plan.moves_to.as_deref() == Some(path.as_str()) {
            plan.edits.get(&finding.path)
        } else if plan.moves_to.is_some() && path == finding.path {
            None
        } else {
            plan.edits.get(&path)
        };
        let mut seen = BTreeSet::new();
        for edit in edited.into_iter().flatten().filter(|edit| edit.shown) {
            if seen.insert((edit.line, edit.text.clone(), edit.was.clone())) {
                rows.push(Row {
                    mark: Mark::Changed,
                    subject: format!("line {}", edit.line),
                    note: format!("{}, was {}", edit.text, edit.was),
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
    Ok(Fixed {
        files,
        errors_left: problems.len(),
    })
}

/// Each error's rule and headline, counted; the finding's own left out.
fn headlines(
    problems: &[Diagnostic],
    finding: Option<&Finding>,
) -> BTreeMap<(String, String), usize> {
    let mut out: BTreeMap<(String, String), usize> = BTreeMap::new();
    for problem in problems {
        if let Some(finding) = finding
            && problem.rule.id == finding.rule.id
            && problem.primary.as_ref().map(|label| &label.span) == Some(&finding.span)
        {
            continue;
        }
        *out.entry((problem.rule.id.to_string(), problem.message.clone()))
            .or_default() += 1;
    }
    out
}

/// Puts back the texts [`apply`] changed, for the editor, which applies
/// the edit itself.
pub fn restore(loaded: &mut Loaded, fixed: &Fixed) {
    let before: BTreeMap<String, Option<String>> = fixed
        .files
        .iter()
        .map(|change| (change.path.clone(), change.before.clone()))
        .collect();
    crate::setting_move::restore_texts(loaded, &before);
}
