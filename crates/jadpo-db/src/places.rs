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
    /// `acceptances/`): application-wide, owned by no domain.
    Root,
    /// Loose files: one implicit domain.
    Loose,
    /// A package (area 11): installed in `packages/<name>/<version>/`, or
    /// the app's own integration in `shared/providers/<name>/<version>/`.
    /// Its names are its own: bare inside it, written through it outside.
    Package(PackageHome),
}

impl Ownership {
    pub fn domain(&self) -> Option<&str> {
        match self {
            Ownership::Domain(name) => Some(name),
            _ => None,
        }
    }

    /// The scope a private name of this owner is unique in: a domain's
    /// name, or a package's name in code (`Postcodes`), which no domain's
    /// can be, since a domain's is lower case.
    pub fn scope(&self) -> Option<&str> {
        match self {
            Ownership::Domain(name) => Some(name),
            Ownership::Package(home) => Some(&home.through),
            _ => None,
        }
    }

    /// The package that owns the code, if a package does.
    pub fn package(&self) -> Option<&PackageHome> {
        match self {
            Ownership::Package(home) => Some(home),
            _ => None,
        }
    }
}

/// Where a package sits (area 11, "Where packages live"), read from its
/// path: `packages/postcodes/v2/` or `shared/providers/acme_couriers/v1/`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageHome {
    /// The package's folder in the project: `packages/postcodes/v2`.
    pub folder: String,
    /// Its name as a folder, `postcodes`.
    pub name: String,
    /// Its version folder, `v2` or `v2024_06_20`.
    pub version: String,
    /// Its name in code, `Postcodes`: the folder's name in
    /// `UpperCamelCase`, which its index has to give.
    pub code: String,
    /// Installed in `packages/`, rather than the app's own in
    /// `shared/providers/`.
    pub installed: bool,
    /// The name the app writes its declarations through: `Postcodes`, or
    /// `Postcodes.v3` while the project holds two versions of it side by
    /// side (area 11, "Where packages live"; chunk 7.1c2). The path alone
    /// gives `code`; [`crate::location`] knows the other versions.
    pub through: String,
}

impl PackageHome {
    /// The package's name and version as code and the lock write it,
    /// `Postcodes.v2`.
    pub fn versioned(&self) -> String {
        format!("{}.{}", self.code, self.version)
    }

    /// The key the app words its text under in the root translation
    /// files, its record and the translation commands: the folder's name,
    /// `postcodes`, or with the version, `postcodes_v3`, while two
    /// versions are held side by side (area 11, "Wording two versions of
    /// a package"; chunk 7.1c3).
    pub fn wording_key(&self) -> String {
        if self.through == self.code {
            self.name.clone()
        } else {
            format!("{}_{}", self.name, self.version)
        }
    }

    /// How a `use` line names it: `packages/postcodes` or
    /// `shared/providers/acme_couriers`.
    pub fn use_path(&self) -> String {
        if self.installed {
            format!("{PACKAGES}/{}", self.name)
        } else {
            format!("shared/{PROVIDERS}/{}", self.name)
        }
    }
}

/// The root folder of installed packages (area 11).
pub const PACKAGES: &str = "packages";

/// The folder of `shared/` that holds the app's own integrations.
pub const PROVIDERS: &str = "providers";

/// A package's index, at its root (area 11).
pub const PACKAGE_INDEX: &str = "package.jadpo";

/// How a package's API is reached, at its root (area 11).
pub const PROVIDER_FILE: &str = "provider.jadpo";

/// The package at the start of a project-relative path, and the rest of
/// the path inside it: `packages/<name>/<version>/...` or
/// `shared/providers/<name>/<version>/...`. A name that isn't
/// `lower_snake_case` or a version that isn't `v` and digits is no
/// package, and the path is somewhere the layout has no place for.
pub fn package_at(path: &str) -> Option<(PackageHome, Vec<&str>)> {
    let parts: Vec<&str> = path.split('/').collect();
    let (installed, name, version, rest) = match parts.as_slice() {
        [PACKAGES, name, version, rest @ ..] if !rest.is_empty() => (true, *name, *version, rest),
        ["shared", PROVIDERS, name, version, rest @ ..] if !rest.is_empty() => {
            (false, *name, *version, rest)
        }
        _ => return None,
    };
    if !is_package_folder(name) || !is_version_label(version) {
        return None;
    }
    let root = if installed {
        format!("{PACKAGES}/{name}/{version}")
    } else {
        format!("shared/{PROVIDERS}/{name}/{version}")
    };
    Some((
        PackageHome {
            folder: root,
            name: name.to_string(),
            version: version.to_string(),
            code: to_upper_camel(name),
            installed,
            through: to_upper_camel(name),
        },
        rest.to_vec(),
    ))
}

/// A package folder's name: `lower_snake_case`.
fn is_package_folder(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !name.contains("__")
        && !name.ends_with('_')
}

/// A version label (area 11, question 16): `v` then digits, in groups
/// joined by single underscores, `v2` or `v2024_06_20`.
pub fn is_version_label(label: &str) -> bool {
    let Some(rest) = label.strip_prefix('v') else {
        return false;
    };
    !rest.is_empty()
        && rest
            .split('_')
            .all(|group| !group.is_empty() && group.bytes().all(|b| b.is_ascii_digit()))
}

/// The `UpperCamelCase` form of a name in any style: `acme_couriers` is
/// `AcmeCouriers`.
pub fn to_upper_camel(name: &str) -> String {
    name_words(name)
        .iter()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Whether a project-relative path inside a package is data rather than
/// source: anything that isn't a `.jadpo` file, an installed package's
/// tests (run when it is published, not in every app's `jadpo test`), and
/// its recipes and sample app, which an app copies in rather than
/// compiles (area 11). Each is read whole, for the package's digest.
pub fn is_package_data(path: &str) -> bool {
    let Some((home, rest)) = package_at(path) else {
        return false;
    };
    let file = rest.last().copied().unwrap_or_default();
    !file.ends_with(".jadpo")
        || matches!(rest.first(), Some(&("recipes" | "sample")))
        || (home.installed && file.ends_with(".test.jadpo"))
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
    /// A provider package's operations, one `resource` per file (area 11).
    Resources,
    /// A provider package's capability mappings, one `provide` per file
    /// (area 11).
    Provides,
}

impl Folder {
    pub const ALL: [Folder; 16] = [
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
        Folder::Resources,
        Folder::Provides,
    ];

    /// The folders a domain may hold.
    pub const DOMAIN: [Folder; 14] = [
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

    /// The folders a package may hold (area 11, "The package format"): laid
    /// out as `shared/` is, with a provider's resources, events, actions,
    /// subscribers and capability mappings. Never `emails/`: email
    /// templates are the app's own.
    pub const PACKAGE: [Folder; 9] = [
        Folder::Types,
        Folder::Failures,
        Folder::Libraries,
        Folder::Translations,
        Folder::Resources,
        Folder::Events,
        Folder::Actions,
        Folder::Subscribers,
        Folder::Provides,
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
            Folder::Resources => "resources",
            Folder::Provides => "provides",
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
            Folder::Resources => *kind == ItemKind::Resource,
            Folder::Provides => *kind == ItemKind::Provide,
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
    /// A package's index, `package.jadpo` at its root (area 11).
    PackageIndex,
    /// A provider package's `provider.jadpo` at its root (area 11).
    Provider,
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
    /// A file in a package folder that isn't a package's place: directly
    /// in `packages/` or a package's own folder, outside a version folder.
    PackageFile,
    /// A folder a package can't hold.
    PackageFolder(String),
    /// A package's `emails/` folder: templates are the app's own (area 11).
    PackageEmails,
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
            match (
                Folder::from_name(folder).filter(|kind| Folder::DOMAIN.contains(kind)),
                rest,
            ) {
                (Some(kind), []) => at(ownership, Place::Folder(kind)),
                (Some(_), _) => at(ownership, Place::Stray(Stray::Nested(folders.join("/")))),
                (None, _) => at(
                    ownership,
                    Place::Stray(Stray::DomainFolder(domain.to_string(), folder.to_string())),
                ),
            }
        }
        _ if package_at(path).is_some() => {
            let Some((home, rest)) = package_at(path) else {
                unreachable!("just matched")
            };
            let ownership = Ownership::Package(home);
            match rest.as_slice() {
                [PACKAGE_INDEX] => at(ownership, Place::PackageIndex),
                [PROVIDER_FILE] => at(ownership, Place::Provider),
                [_] => at(ownership, Place::Stray(Stray::PackageFile)),
                ["emails", ..] => at(ownership, Place::Stray(Stray::PackageEmails)),
                [folder, inner @ ..] => {
                    match (
                        Folder::from_name(folder).filter(|kind| Folder::PACKAGE.contains(kind)),
                        inner,
                    ) {
                        (Some(kind), [_]) => at(ownership, Place::Folder(kind)),
                        (Some(_), _) => {
                            at(ownership, Place::Stray(Stray::Nested(folders.join("/"))))
                        }
                        (None, _) => at(
                            ownership,
                            Place::Stray(Stray::PackageFolder(folder.to_string())),
                        ),
                    }
                }
                [] => at(ownership, Place::Stray(Stray::PackageFile)),
            }
        }
        [PACKAGES, ..] | ["shared", PROVIDERS, ..] => {
            at(Ownership::Root, Place::Stray(Stray::PackageFile))
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
