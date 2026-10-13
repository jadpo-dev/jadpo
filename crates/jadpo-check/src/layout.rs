//! Area 1's layout rules, checked from each file's path and item tree:
//! where a `.jadpo` file may sit, which declarations each folder holds,
//! file names, test subjects and route prefixes.

use std::collections::BTreeSet;

use jadpo_db::{
    Base, Db, File, Folder, Item, ItemKind, Layout, Location, Ownership, Place, Project, RootFile,
    Stray, item_tree, location,
};
use jadpo_diag::{Diagnostic, rule};
use jadpo_syntax::ast::{self, AstNode, Decl};

use crate::case::to_snake;
use crate::tree::{self, file_span, name_span, node_span};

/// The declaration kind a fixed folder is named after.
fn holds(folder: Folder, kind: &ItemKind) -> bool {
    match folder {
        Folder::Entities => *kind == ItemKind::Entity,
        Folder::Types => matches!(
            kind,
            ItemKind::Type(base) if !matches!(base, Base::Failure | Base::Event | Base::EmailMessage)
        ),
        Folder::Failures => *kind == ItemKind::Type(Base::Failure),
        Folder::Actions => *kind == ItemKind::Action,
        Folder::Queries => *kind == ItemKind::Query,
        Folder::Libraries => *kind == ItemKind::Library,
        Folder::Routes => *kind == ItemKind::Routes,
        Folder::Schedules => *kind == ItemKind::Schedule,
        Folder::Events => *kind == ItemKind::Type(Base::Event),
        Folder::Subscribers => *kind == ItemKind::Subscriber,
        Folder::Emails => *kind == ItemKind::Type(Base::EmailMessage),
        Folder::Fixtures => *kind == ItemKind::Fixture,
    }
}

/// What a folder holds, in the reader's words.
fn holding(folder: Folder) -> &'static str {
    match folder {
        Folder::Entities => "one entity per file",
        Folder::Types => "types shared by several files in the domain",
        Folder::Failures => "one failure per file",
        Folder::Actions => "one action per file",
        Folder::Queries => "one query per file",
        Folder::Libraries => "libraries of helper functions",
        Folder::Routes => "one route group per file",
        Folder::Schedules => "one schedule per file",
        Folder::Events => "one event per file",
        Folder::Subscribers => "one subscriber per file",
        Folder::Emails => "one email per file",
        Folder::Fixtures => "test data shared by the domain's tests",
    }
}

/// Supporting declarations may sit beside a file's main one: types and
/// helper functions used only by that file.
fn supporting(kind: &ItemKind) -> bool {
    matches!(kind, ItemKind::Function) || holds(Folder::Types, kind)
}

/// Declarations a test file may hold.
fn in_tests(kind: &ItemKind) -> bool {
    matches!(kind, ItemKind::Test | ItemKind::Fixture) || supporting(kind)
}

/// The folder a declaration of this kind lives in.
fn home(kind: &ItemKind) -> Option<Folder> {
    Folder::ALL
        .into_iter()
        .find(|folder| holds(*folder, kind))
        .or(match kind {
            ItemKind::Function => Some(Folder::Libraries),
            _ => None,
        })
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Where a declaration belongs, written for a `next:` line.
fn destination(ownership: &Ownership, item: &Item) -> Option<String> {
    let file = to_snake(&item.name);
    let domain = match ownership {
        Ownership::Domain(domain) => format!("domains/{domain}"),
        Ownership::Shared => "shared".to_string(),
        _ => "domains/<domain>".to_string(),
    };
    match &item.kind {
        ItemKind::Seed => Some(format!("`database/seeds/{file}.jadpo`")),
        ItemKind::Package | ItemKind::Resource | ItemKind::Provide => {
            Some("`providers/`".to_string())
        }
        ItemKind::Singleton | ItemKind::Test => None,
        ItemKind::Function => Some(format!("a library in `{domain}/libraries/`")),
        kind => home(kind).map(|folder| format!("`{domain}/{}/{file}.jadpo`", folder.name())),
    }
}

/// The layout problems of one file.
#[salsa::tracked(returns(ref))]
pub fn layout_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    if project.layout(db) == Layout::Loose {
        return Vec::new();
    }
    let path = file.path(db);
    let location = location(db, project, file);
    let items = &item_tree(db, file).items;
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    match &location.place {
        Place::Stray(stray) => out.push(unknown_location(path, stray, location, items)),
        Place::RootFile(root) => {
            for item in items {
                if !matches!(item.kind, ItemKind::Singleton | ItemKind::Type(_)) {
                    out.push(in_root_file(path, *root, &source, item));
                }
            }
        }
        Place::Folder(folder) if location.test => {
            for item in items.iter().filter(|item| !in_tests(&item.kind)) {
                out.push(not_test_content(path, location, &source, item));
            }
            let subject = path.replace(".test.jadpo", ".jadpo");
            let known: BTreeSet<&String> = project.files(db).iter().map(|f| f.path(db)).collect();
            if !known.contains(&subject) {
                out.push(without_subject(path, location, *folder));
            }
        }
        Place::Folder(folder) => out.extend(folder_rules(path, *folder, location, &source, items)),
        Place::Tests => {
            for item in items.iter().filter(|item| !in_tests(&item.kind)) {
                out.push(not_test_content(path, location, &source, item));
            }
        }
        Place::Seeds => {
            for item in items {
                if item.kind == ItemKind::Seed {
                    if to_snake(&item.name) != location.stem {
                        out.push(misnamed(path, &source, item));
                    }
                } else if !supporting(&item.kind) {
                    out.push(misplaced(path, "database/seeds", location, &source, item));
                }
            }
        }
        _ => {}
    }
    out
}

fn folder_rules(
    path: &str,
    folder: Folder,
    location: &Location,
    source: &ast::SourceFile,
    items: &[Item],
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let main: Vec<&Item> = items
        .iter()
        .filter(|item| holds(folder, &item.kind))
        .collect();
    let primary = main
        .iter()
        .find(|item| to_snake(&item.name) == location.stem)
        .or(main.first())
        .copied();
    let place = match &location.ownership {
        Ownership::Domain(domain) => format!("domains/{domain}/{}", folder.name()),
        _ => format!("shared/{}", folder.name()),
    };
    if let Some(primary) = primary {
        if to_snake(&primary.name) != location.stem {
            out.push(misnamed(path, source, primary));
        }
        if folder == Folder::Routes {
            out.extend(route_prefix(path, location, source, primary));
        }
    }
    for item in items {
        let is_primary = primary.is_some_and(|p| p.index == item.index);
        if is_primary {
            continue;
        }
        if holds(folder, &item.kind) {
            if folder != Folder::Types {
                out.push(own_file(path, &place, folder, source, item));
            }
        } else if !supporting(&item.kind) || primary.is_none() {
            out.push(misplaced(path, folder.name(), location, source, item));
        }
    }
    out
}

fn unknown_location(path: &str, stray: &Stray, location: &Location, items: &[Item]) -> Diagnostic {
    let file = path.rsplit('/').next().unwrap_or(path);
    let folders = Folder::ALL
        .iter()
        .map(|folder| format!("`{}/`", folder.name()))
        .collect::<Vec<_>>();
    let (message, reason) = match stray {
        Stray::RootFile => (
            format!("`{file}` isn't one of the root files"),
            format!(
                "The root holds only {}; all other code lives in a domain under `domains/`.",
                crate::and_list(
                    &RootFile::ALL
                        .iter()
                        .map(|root| format!("`{}`", root.file_name()))
                        .collect::<Vec<_>>()
                )
            ),
        ),
        Stray::RootFolder(folder) => (
            format!("`{folder}/` isn't a place for Jadpo code"),
            "Code lives in `domains/` and `shared/`; the other root folders are `tests/`, `database/`, `providers/` and `acceptances/`, and `api/` and `translations/` hold no `.jadpo` files.".to_string(),
        ),
        Stray::DomainsFile => (
            format!("`{file}` is outside any domain"),
            "Everything under `domains/` belongs to a domain: `domains/<domain>/<folder>/<file>.jadpo`.".to_string(),
        ),
        Stray::DomainFile(domain) => (
            format!("`{file}` sits directly in domain `{domain}`"),
            "A domain keeps its code in fixed folders; the only file directly in it is `monitoring.jadpo`.".to_string(),
        ),
        Stray::DomainFolder(_, folder) => (
            format!("`{folder}/` isn't a folder a domain has"),
            format!(
                "A domain holds only {}, each named after what its files declare.",
                crate::and_list(&folders)
            ),
        ),
        Stray::SharedFolder(folder) => (
            match folder {
                Some(folder) => format!("`shared/{folder}/` isn't part of `shared/`"),
                None => format!("`{file}` sits directly in `shared/`"),
            },
            "`shared/` holds only `types/`, `libraries/` and `failures/`: code every domain may use.".to_string(),
        ),
        Stray::Nested(folder) => (
            format!("`{folder}/` is a folder inside a fixed folder"),
            "Domains are one level deep and their folders have no subfolders, so a file's path follows from its domain and what it declares.".to_string(),
        ),
        Stray::NotATest => (
            format!("`{file}` isn't a test file"),
            "The root `tests/` folder holds scenarios across domains, each in a `<name>.test.jadpo` file.".to_string(),
        ),
    };
    let main = items
        .iter()
        .find(|item| !supporting(&item.kind))
        .or(items.first());
    let next = match main.and_then(|item| destination(&location.ownership, item)) {
        Some(place) if !matches!(stray, Stray::NotATest) => format!("Move the file to {place}"),
        _ if matches!(stray, Stray::NotATest) => {
            format!(
                "Rename it `{}.test.jadpo`, or move its code into a domain",
                location.stem
            )
        }
        _ => "Move the file into the folder for what it declares".to_string(),
    };
    Diagnostic::new(&rule::LAYOUT_UNKNOWN_LOCATION, message)
        .at(file_span(path), "")
        .reason(reason)
        .next(next)
}

fn in_root_file(path: &str, root: RootFile, source: &ast::SourceFile, item: &Item) -> Diagnostic {
    let kind = item.kind.word();
    let target = home(&item.kind)
        .map(|folder| format!("`domains/<domain>/{}/`", folder.name()))
        .unwrap_or_else(|| "a domain".to_string());
    Diagnostic::new(
        &rule::LAYOUT_WRONG_FOLDER,
        format!(
            "{} `{}` is in `{}`",
            capitalised(kind),
            item.name,
            root.file_name()
        ),
    )
    .at(
        name_span(source, path, item.index, None),
        "belongs in a domain",
    )
    .reason(
        "The root files hold the application's settings; code lives in a domain under `domains/`.",
    )
    .next(format!("Move `{}` to {target}", item.name))
}

fn misplaced(
    path: &str,
    folder: &str,
    location: &Location,
    source: &ast::SourceFile,
    item: &Item,
) -> Diagnostic {
    let kind = item.kind.word();
    let target = home(&item.kind);
    let (label, rest) = match target {
        Some(Folder::Libraries) if item.kind == ItemKind::Function => (
            "belongs beside its user".to_string(),
            "a helper function lives beside the one file that uses it, or in a library once several do".to_string(),
        ),
        Some(Folder::Types) => (
            "belongs beside its user".to_string(),
            "a type lives beside the one file that uses it, or in `types/` once several do".to_string(),
        ),
        Some(target) => (
            format!("belongs in `{}/`", target.name()),
            format!("every {kind} has its own file in `{}/`", target.name()),
        ),
        None => (
            "belongs elsewhere".to_string(),
            format!("{} doesn't belong in a domain's folders", item.kind.with_a()),
        ),
    };
    let what = Folder::from_name(folder).map_or("seeds", holding);
    let next = match destination(&location.ownership, item) {
        Some(place) => format!("Move `{}` to {place}", item.name),
        None => format!("Move `{}` out of `{folder}/`", item.name),
    };
    Diagnostic::new(
        &rule::LAYOUT_WRONG_FOLDER,
        format!("{} `{}` is in `{folder}/`", capitalised(kind), item.name),
    )
    .at(name_span(source, path, item.index, None), label)
    .reason(format!("`{folder}/` holds {what}; {rest}."))
    .next(next)
}

fn not_test_content(
    path: &str,
    location: &Location,
    source: &ast::SourceFile,
    item: &Item,
) -> Diagnostic {
    let kind = item.kind.word();
    let next = match destination(&location.ownership, item) {
        Some(place) => format!("Move `{}` to {place}", item.name),
        None => format!("Move `{}` out of the test file", item.name),
    };
    Diagnostic::new(
        &rule::LAYOUT_WRONG_FOLDER,
        format!("{} `{}` is in a test file", capitalised(kind), item.name),
    )
    .at(name_span(source, path, item.index, None), "not test code")
    .reason("A test file holds tests, with the fixtures, types and helper functions only they use.")
    .next(next)
}

fn misnamed(path: &str, source: &ast::SourceFile, item: &Item) -> Diagnostic {
    let file = format!("{}.jadpo", to_snake(&item.name));
    let current = path.rsplit('/').next().unwrap_or(path);
    Diagnostic::new(
        &rule::LAYOUT_FILE_NAME_MISMATCH,
        format!("`{}` is in `{current}`", item.name),
    )
    .at(
        name_span(source, path, item.index, None),
        format!("file name should be `{file}`"),
    )
    .reason(
        "A file is named after the declaration it holds, so a search for a name finds its file.",
    )
    .next(format!("Rename `{current}` to `{file}`"))
}

fn own_file(
    path: &str,
    place: &str,
    folder: Folder,
    source: &ast::SourceFile,
    item: &Item,
) -> Diagnostic {
    let kind = item.kind.word();
    Diagnostic::new(
        &rule::LAYOUT_FILE_NAME_MISMATCH,
        format!("`{}` needs a file of its own", item.name),
    )
    .at(
        name_span(source, path, item.index, None),
        format!("a second {kind} in this file"),
    )
    .reason(format!(
        "Every {kind} has its own file in `{}/`, named after it.",
        folder.name()
    ))
    .next(format!(
        "Move `{}` to `{place}/{}.jadpo`",
        item.name,
        to_snake(&item.name)
    ))
}

fn without_subject(path: &str, location: &Location, folder: Folder) -> Diagnostic {
    let file = path.rsplit('/').next().unwrap_or(path);
    let subject = format!("{}.jadpo", location.stem);
    Diagnostic::new(
        &rule::LAYOUT_TEST_WITHOUT_SUBJECT,
        format!("`{file}` has no `{subject}` beside it"),
    )
    .at(file_span(path), "")
    .reason(format!(
        "A test file sits beside the file it tests, named after it; scenarios across domains go in the root `tests/` folder. `{}/` has no `{subject}`.",
        folder.name()
    ))
    .next("Rename it after the file it tests, or move it to `tests/`")
}

fn route_prefix(
    path: &str,
    location: &Location,
    source: &ast::SourceFile,
    item: &Item,
) -> Option<Diagnostic> {
    let segments = item.prefix.as_ref()?;
    if segments.is_empty() {
        return None;
    }
    let expected = segments
        .iter()
        .map(|segment| segment.replace('-', "_"))
        .collect::<Vec<_>>()
        .join("_");
    if expected == location.stem {
        return None;
    }
    let Some(Decl::RoutesDecl(group)) = tree::decl_at(source, item.index) else {
        return None;
    };
    let (_, member) = tree::members(group.member_list())
        .into_iter()
        .find(|(name, _)| tree::name_text(name).as_deref() == Some("prefix"))?;
    let written = member
        .value()
        .map(|value| tree::written(value.syntax()))
        .unwrap_or_default();
    let span = member.value().map_or_else(
        || node_span(path, member.syntax()),
        |value| node_span(path, value.syntax()),
    );
    let file = path.rsplit('/').next().unwrap_or(path);
    Some(
        Diagnostic::new(
            &rule::LAYOUT_ROUTE_PREFIX_MISMATCH,
            format!("Prefix `{written}` doesn't match `{file}`"),
        )
        .at(span, format!("gives `{expected}.jadpo`"))
        .reason(format!(
            "A route group's file is named after its prefix's fixed segments joined with `_`, so a URL leads to one file: `{written}` lives in `{expected}.jadpo`."
        ))
        .next(format!(
            "Change the prefix to match `{file}`, or rename the file `{expected}.jadpo` and the group `{}`",
            crate::case::to_camel(&expected)
        )),
    )
}
