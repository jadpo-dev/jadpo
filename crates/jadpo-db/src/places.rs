//! Where a file sits in the fixed layout (area 1): which domain owns it,
//! what its folder is for, and what its name says. Read from the path
//! alone, so it never needs the file's text.

use crate::Layout;
use crate::items::{Base, ItemKind};

/// Who owns the code in a file, for visibility.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Ownership {
    /// A business area under `domains/`.
    Domain(String),
    /// `shared/`: pure types, libraries and failures every domain may use.
    Shared,
    /// The root files and the other root folders (`tests/`, `database/`,
    /// `providers/`, `acceptances/`): application-wide, owned by no domain.
    Root,
    /// Loose files: one implicit domain.
    Loose,
}

impl Ownership {
    pub fn domain(&self) -> Option<&str> {
        match self {
            Ownership::Domain(name) => Some(name),
            _ => None,
        }
    }
}

/// The root files the layout fixes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RootFile {
    App,
    Config,
    Authentication,
    Policy,
    Flags,
}

impl RootFile {
    pub const ALL: [RootFile; 5] = [
        RootFile::App,
        RootFile::Config,
        RootFile::Authentication,
        RootFile::Policy,
        RootFile::Flags,
    ];

    pub fn file_name(self) -> &'static str {
        match self {
            RootFile::App => "app.jadpo",
            RootFile::Config => "config.jadpo",
            RootFile::Authentication => "authentication.jadpo",
            RootFile::Policy => "policy.jadpo",
            RootFile::Flags => "flags.jadpo",
        }
    }
}

/// A domain's fixed folders, each named after what its files declare.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Folder {
    Entities,
    Types,
    Failures,
    Actions,
    Queries,
    Libraries,
    Routes,
    Schedules,
    Events,
    Subscribers,
    Emails,
    /// `type X = Message` files and the `translations { }` language files
    /// (area 10).
    Translations,
    /// `type X = Prompt` files, each with its `<name>.prompt` text beside
    /// it (area 21).
    Prompts,
    Fixtures,
}

impl Folder {
    pub const ALL: [Folder; 14] = [
        Folder::Entities,
        Folder::Types,
        Folder::Failures,
        Folder::Actions,
        Folder::Queries,
        Folder::Libraries,
        Folder::Routes,
        Folder::Schedules,
        Folder::Events,
        Folder::Subscribers,
        Folder::Emails,
        Folder::Translations,
        Folder::Prompts,
        Folder::Fixtures,
    ];

    /// The folders `shared/` may hold.
    pub const SHARED: [Folder; 5] = [
        Folder::Types,
        Folder::Libraries,
        Folder::Failures,
        Folder::Translations,
        Folder::Prompts,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Folder::Entities => "entities",
            Folder::Types => "types",
            Folder::Failures => "failures",
            Folder::Actions => "actions",
            Folder::Queries => "queries",
            Folder::Libraries => "libraries",
            Folder::Routes => "routes",
            Folder::Schedules => "schedules",
            Folder::Events => "events",
            Folder::Subscribers => "subscribers",
            Folder::Emails => "emails",
            Folder::Translations => "translations",
            Folder::Prompts => "prompts",
            Folder::Fixtures => "fixtures",
        }
    }

    pub fn from_name(name: &str) -> Option<Folder> {
        Folder::ALL.into_iter().find(|folder| folder.name() == name)
    }

    /// Whether the folder is named after declarations of this kind: the
    /// kind of each file's primary declaration there.
    pub fn holds(self, kind: &ItemKind) -> bool {
        match self {
            Folder::Entities => *kind == ItemKind::Entity,
            Folder::Types => matches!(
                kind,
                ItemKind::Type(base) if !matches!(base, Base::Failure | Base::Event | Base::EmailMessage | Base::Message | Base::Prompt)
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
            // A message, or a language file's `translations { }`.
            Folder::Translations => {
                matches!(kind, ItemKind::Type(Base::Message) | ItemKind::Singleton)
            }
            Folder::Prompts => *kind == ItemKind::Type(Base::Prompt),
            Folder::Fixtures => *kind == ItemKind::Fixture,
        }
    }
}

/// What a file's place in the layout is for.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Place {
    RootFile(RootFile),
    /// `domains/<domain>/<folder>/` or `shared/<folder>/`.
    Folder(Folder),
    /// `domains/<domain>/monitoring.jadpo` (area 16).
    Monitoring,
    /// The root `tests/` folder: scenarios across domains.
    Tests,
    /// `database/seeds/`.
    Seeds,
    /// `database/migrations/<migration>/`: backfills written in Jadpo.
    Migration,
    /// `providers/`: provider packages (area 11).
    Providers,
    /// `acceptances/` (area 18).
    Acceptances,
    /// The root `translations/` folder: the app's wording for text it did
    /// not write, one `translations { }` file for each locale, grouped by
    /// owner (area 10).
    RootTranslations,
    /// A loose file: no layout.
    Loose,
    /// Somewhere the layout has no place for a `.jadpo` file.
    Stray(Stray),
}

/// Why a path is outside the layout, for the error that says so.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Stray {
    /// A `.jadpo` file at the root that isn't one of the root files.
    RootFile,
    /// A root folder the layout doesn't have, or one that holds no Jadpo.
    RootFolder(String),
    /// A file directly in `domains/`, outside any domain.
    DomainsFile,
    /// A file directly in a domain folder that isn't `monitoring.jadpo`.
    DomainFile(String),
    /// A folder a domain can't hold.
    DomainFolder(String, String),
    /// A folder `shared/` can't hold, or a file directly in `shared/`.
    SharedFolder(Option<String>),
    /// A folder inside one of the fixed folders.
    Nested(String),
    /// A file in `tests/` that isn't a test.
    NotATest,
}

/// A file's location: who owns it, its place, and the name its path gives it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Location {
    pub ownership: Ownership,
    pub place: Place,
    /// The file name without `.jadpo`, and without `.test` for a test.
    pub stem: String,
    /// `<name>.test.jadpo`.
    pub test: bool,
}

impl Location {
    /// The fixed folder the file is in, if any.
    pub fn folder(&self) -> Option<Folder> {
        match self.place {
            Place::Folder(folder) => Some(folder),
            _ => None,
        }
    }
}

/// Root folders whose files are not Jadpo source.
const RECORD_FOLDERS: [&str; 2] = ["api", "build"];

/// Reads a project-relative path (`/` separators) against the layout.
pub fn locate(path: &str, layout: Layout) -> Location {
    let parts: Vec<&str> = path.split('/').collect();
    let file = parts.last().copied().unwrap_or("");
    let name = file.strip_suffix(".jadpo").unwrap_or(file);
    let (stem, test) = match name.strip_suffix(".test") {
        Some(stem) => (stem.to_string(), true),
        None => (name.to_string(), false),
    };
    let at = |ownership: Ownership, place: Place| Location {
        ownership,
        place,
        stem: stem.clone(),
        test,
    };
    if layout == Layout::Loose {
        return at(Ownership::Loose, Place::Loose);
    }
    let folders = &parts[..parts.len().saturating_sub(1)];
    match folders {
        [] => match RootFile::ALL
            .into_iter()
            .find(|root| root.file_name() == file)
        {
            Some(root) => at(Ownership::Root, Place::RootFile(root)),
            None => at(Ownership::Root, Place::Stray(Stray::RootFile)),
        },
        ["domains"] => at(Ownership::Root, Place::Stray(Stray::DomainsFile)),
        ["domains", domain] => {
            let ownership = Ownership::Domain(domain.to_string());
            if file == "monitoring.jadpo" {
                at(ownership, Place::Monitoring)
            } else {
                at(
                    ownership,
                    Place::Stray(Stray::DomainFile(domain.to_string())),
                )
            }
        }
        ["domains", domain, folder, rest @ ..] => {
            let ownership = Ownership::Domain(domain.to_string());
            match (Folder::from_name(folder), rest) {
                (Some(kind), []) => at(ownership, Place::Folder(kind)),
                (Some(_), _) => at(ownership, Place::Stray(Stray::Nested(folders.join("/")))),
                (None, _) => at(
                    ownership,
                    Place::Stray(Stray::DomainFolder(domain.to_string(), folder.to_string())),
                ),
            }
        }
        ["shared"] => at(Ownership::Shared, Place::Stray(Stray::SharedFolder(None))),
        ["shared", folder, rest @ ..] => {
            match (
                Folder::from_name(folder).filter(|kind| Folder::SHARED.contains(kind)),
                rest,
            ) {
                (Some(kind), []) => at(Ownership::Shared, Place::Folder(kind)),
                (Some(_), _) => at(
                    Ownership::Shared,
                    Place::Stray(Stray::Nested(folders.join("/"))),
                ),
                (None, _) => at(
                    Ownership::Shared,
                    Place::Stray(Stray::SharedFolder(Some(folder.to_string()))),
                ),
            }
        }
        ["tests"] if test => at(Ownership::Root, Place::Tests),
        ["tests"] => at(Ownership::Root, Place::Stray(Stray::NotATest)),
        ["tests", ..] => at(
            Ownership::Root,
            Place::Stray(Stray::Nested(folders.join("/"))),
        ),
        ["database", "seeds"] => at(Ownership::Root, Place::Seeds),
        ["database", "migrations", _] => at(Ownership::Root, Place::Migration),
        ["providers", ..] => at(Ownership::Root, Place::Providers),
        ["acceptances"] => at(Ownership::Root, Place::Acceptances),
        ["translations"] => at(Ownership::Root, Place::RootTranslations),
        [first, ..] if RECORD_FOLDERS.contains(first) => at(
            Ownership::Root,
            Place::Stray(Stray::RootFolder(first.to_string())),
        ),
        _ => at(
            Ownership::Root,
            Place::Stray(Stray::RootFolder(folders.join("/"))),
        ),
    }
}

/// The words of a name in any style: `HTTPStatus` and `http_status` are
/// `http status`.
pub fn name_words(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' || c == '-' {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        let previous = i.checked_sub(1).map(|j| chars[j]);
        let next = chars.get(i + 1).copied();
        let boundary = c.is_ascii_uppercase()
            && !current.is_empty()
            && (previous.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit())
                || (previous.is_some_and(|p| p.is_ascii_uppercase())
                    && next.is_some_and(|n| n.is_ascii_lowercase())));
        if boundary {
            words.push(std::mem::take(&mut current));
        }
        current.push(c.to_ascii_lowercase());
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// The `lower_snake_case` form: a file name, or the spelling of a value.
pub fn to_snake(name: &str) -> String {
    name_words(name).join("_")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(path: &str) -> Place {
        locate(path, Layout::Project).place
    }

    #[test]
    fn the_fixed_layout_is_read_from_paths() {
        assert_eq!(place("app.jadpo"), Place::RootFile(RootFile::App));
        assert_eq!(place("flags.jadpo"), Place::RootFile(RootFile::Flags));
        assert_eq!(
            place("domains/planning/actions/complete_todo.jadpo"),
            Place::Folder(Folder::Actions)
        );
        assert_eq!(
            place("domains/planning/monitoring.jadpo"),
            Place::Monitoring
        );
        assert_eq!(
            place("shared/libraries/rounding.jadpo"),
            Place::Folder(Folder::Libraries)
        );
        assert_eq!(place("tests/checkout.test.jadpo"), Place::Tests);
        assert_eq!(place("database/seeds/local_demo.jadpo"), Place::Seeds);
        let location = locate("domains/planning/entities/todo.test.jadpo", Layout::Project);
        assert!(location.test);
        assert_eq!(location.stem, "todo");
        assert_eq!(location.ownership, Ownership::Domain("planning".into()));
    }

    #[test]
    fn anything_else_is_stray() {
        assert_eq!(place("utils.jadpo"), Place::Stray(Stray::RootFile));
        assert_eq!(
            place("domains/planning/helpers/x.jadpo"),
            Place::Stray(Stray::DomainFolder("planning".into(), "helpers".into()))
        );
        assert_eq!(
            place("domains/planning/actions/old/x.jadpo"),
            Place::Stray(Stray::Nested("domains/planning/actions/old".into()))
        );
        assert_eq!(
            place("shared/actions/x.jadpo"),
            Place::Stray(Stray::SharedFolder(Some("actions".into())))
        );
        assert_eq!(place("tests/notes.jadpo"), Place::Stray(Stray::NotATest));
        assert_eq!(
            place("src/x.jadpo"),
            Place::Stray(Stray::RootFolder("src".into()))
        );
    }

    #[test]
    fn loose_files_have_no_layout() {
        let location = locate("anything/at/all.jadpo", Layout::Loose);
        assert_eq!(location.ownership, Ownership::Loose);
        assert_eq!(location.place, Place::Loose);
    }
}
