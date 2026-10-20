//! What a language file can word (area 10, "Translations as messages"):
//! the messages a folder declares, the enums whose values get wording, and
//! the email subjects. Each is read once into what its wording is allowed
//! to be: a plain string, one string for each value of an enum, or one for
//! each plural form, and the fields a wording may write.

use jadpo_db::{
    Base, Db, File, Folder, ItemKind, Ownership, Place, Project, Symbol, Symbols, symbols,
};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_std::translation::{ClientFormat, ParamKind, TranslationParam};
use jadpo_syntax::ast::{self, AstNode, Decl};

use crate::case::to_snake;
use crate::lookups::{Lookup, Name, file_at};
use crate::tree::{self, node_span};
use crate::types::{Def, FieldDef, Prim, Ty, TypeView};

/// What a subject is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `type X = Message { }`: always worded, in every complete locale.
    Message,
    /// An enum with wording for its values: worded once any language file
    /// words it.
    Enum,
    /// An email's subject line: worded in the language files, with the
    /// email's fields as placeholders, when the email machinery asks.
    Email,
    /// A failure's text for client apps, keyed by its code: worded in every
    /// supported locale when a route can answer with it, and sent to
    /// clients with the response's code, never written by the server.
    Failure,
}

impl Kind {
    /// The word for it in an error.
    pub fn word(self) -> &'static str {
        match self {
            Kind::Message => "message",
            Kind::Enum => "enum",
            Kind::Email => "email",
            Kind::Failure => "failure",
        }
    }
}

/// One parameter of the call a subject becomes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamSpec {
    pub name: String,
    pub kind: ParamKind,
    /// For an enum's value: the enum.
    pub enum_type: Option<String>,
}

/// One value of the enum a wording is chosen by, with the payload fields
/// its wording may also write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantInfo {
    pub name: String,
    /// A payload field and, when a wording can write it, how.
    pub payload: Vec<(String, Option<ParamKind>)>,
}

/// How many wordings a language gives a subject.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    /// One.
    Plain,
    /// One for each value of an enum.
    Variants(Vec<VariantInfo>),
    /// One for each plural form the language needs.
    Forms,
}

/// A message, an enum with wording or an email subject.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subject {
    /// The member name that words it: the type's name in lower_snake_case.
    pub key: String,
    pub type_name: String,
    pub kind: Kind,
    pub symbol: Symbol,
    pub name_span: Span,
    /// The call's parameters, after `locale:`.
    pub params: Vec<ParamSpec>,
    /// What a wording may write as `{name}`.
    pub placeholders: Vec<(String, ParamKind)>,
    pub mode: Mode,
    /// For a failure: how a client formats each client field that needs it
    /// before filling its placeholder.
    pub formats: Vec<(String, ClientFormat)>,
    /// A mistake in its declaration, reported; nothing is checked against
    /// it.
    pub broken: bool,
    /// For one of Jadpo's own messages: the feature that switches it on.
    /// Such a subject has no declaration in the project.
    pub builtin: Option<jadpo_std::jadpo_text::Feature>,
    /// The `///` description of its type: what a translator is told it is
    /// for.
    pub description: Option<String>,
}

impl Subject {
    /// Whether code can call it: emails are worded for the email's own
    /// machinery and failures for client apps.
    pub fn callable(&self) -> bool {
        !matches!(self.kind, Kind::Email | Kind::Failure) && !self.broken
    }

    pub fn call_params(&self) -> Vec<TranslationParam> {
        self.params
            .iter()
            .map(|param| TranslationParam {
                name: param.name.clone(),
                kind: param.kind,
            })
            .collect()
    }
}

/// Every subject of a scope, with the mistakes in how they are declared.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Subjects {
    pub list: Vec<Subject>,
    pub problems: Vec<Diagnostic>,
}

impl Subjects {
    pub fn get(&self, key: &str) -> Option<&Subject> {
        self.list.iter().find(|subject| subject.key == key)
    }
}

/// The scope an owner's translations are in: the domain's name, `shared`,
/// or a library package's wording key, which no domain can share
/// (`package.name_taken`) and which names its group in the root file: its
/// folder's name (`postcodes`), or with the version (`postcodes_v3`)
/// while two versions are held side by side (chunk 7.1c3).
pub fn scope_of(ownership: &Ownership) -> Option<String> {
    match ownership {
        Ownership::Domain(domain) => Some(domain.clone()),
        Ownership::Shared => Some(SHARED.to_string()),
        Ownership::Package(home) => Some(home.wording_key()),
        _ => None,
    }
}

/// A symbol's name as its own scope writes it: a package's declarations
/// are known through the package (`Postcodes.PostcodeHint`), and are
/// `PostcodeHint` inside it.
pub fn bare_name(symbol: &Symbol) -> &str {
    match &symbol.ownership {
        Ownership::Package(home) => symbol
            .name
            .strip_prefix(home.through.as_str())
            .and_then(|rest| rest.strip_prefix('.'))
            .unwrap_or(&symbol.name),
        _ => &symbol.name,
    }
}

/// A symbol's name as the checker's failure table and the app know it: a
/// package's declarations are written through the package,
/// `Postcodes.InvalidPostcode`.
pub fn known_name(symbol: &Symbol) -> String {
    match &symbol.ownership {
        Ownership::Package(home) => format!("{}.{}", home.through, bare_name(symbol)),
        _ => symbol.name.clone(),
    }
}

/// The library packages that carry translations (area 10, "Jadpo's and
/// packages' text"), each as its scope and where it is, in scope order: a
/// message in its `translations/` or a language file there. A provider
/// package's are `package.provider_translations`, and words nothing.
#[salsa::tracked(returns(ref))]
pub fn package_scopes(db: &dyn Db, project: Project) -> Vec<(String, jadpo_db::PackageHome)> {
    let mut found: std::collections::BTreeMap<String, jadpo_db::PackageHome> =
        std::collections::BTreeMap::new();
    for &file in project.files(db) {
        let location = jadpo_db::location(db, project, file);
        if let Ownership::Package(home) = &location.ownership
            && matches!(location.place, Place::Folder(Folder::Translations))
            && !location.test
        {
            found
                .entry(home.wording_key())
                .or_insert_with(|| home.clone());
        }
    }
    found.retain(|_, home| {
        let provider = format!("{}/{}", home.folder, jadpo_db::PROVIDER_FILE);
        file_at(db, project, Name::new(db, provider)).is_none()
    });
    found.into_iter().collect()
}

/// The library package whose translations are in `scope`, if one is.
pub fn package_of(db: &dyn Db, project: Project, scope: &str) -> Option<jadpo_db::PackageHome> {
    package_scopes(db, project)
        .iter()
        .find(|(name, _)| name == scope)
        .map(|(_, home)| home.clone())
}

/// The scope of `shared/translations/`.
pub const SHARED: &str = "shared";

/// Every scope of the application's own that declares a message or holds
/// a language file, in name order: an enum or an email is worded only where
/// a language file says so.
pub fn scopes(db: &dyn Db, project: Project) -> Vec<String> {
    let mut found: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    // A library package's translations are checked by their own rules,
    // against the package's locales and the app's root file
    // ([`package_scopes`]).
    for symbol in all_symbols(symbols(db, project)) {
        if subject_kind(symbol) == Some(Kind::Message)
            && symbol.ownership.package().is_none()
            && let Some(scope) = scope_of(&symbol.ownership)
        {
            found.insert(scope);
        }
    }
    for &file in project.files(db) {
        let location = jadpo_db::location(db, project, file);
        if matches!(location.place, Place::Folder(Folder::Translations))
            && !location.test
            && location.ownership.package().is_none()
            && let Some(scope) = scope_of(&location.ownership)
        {
            found.insert(scope);
        }
    }
    found.into_iter().collect()
}

fn all_symbols(table: &Symbols) -> impl Iterator<Item = &Symbol> {
    table.declarations.values().flatten()
}

/// What a symbol is as a subject, if it is one.
fn subject_kind(symbol: &Symbol) -> Option<Kind> {
    if symbol.test || symbol.library.is_some() {
        return None;
    }
    // The not-found failure the compiler provides for an entity is worded
    // like one the project declares; nothing else it provides is a subject.
    if symbol.provided.is_some() {
        return matches!(symbol.kind, ItemKind::Type(Base::Failure)).then_some(Kind::Failure);
    }
    match &symbol.kind {
        ItemKind::Type(Base::Message) if symbol.place == Place::Folder(Folder::Translations) => {
            Some(Kind::Message)
        }
        ItemKind::Type(Base::EmailMessage) => Some(Kind::Email),
        ItemKind::Type(Base::Failure) => Some(Kind::Failure),
        ItemKind::Type(Base::Enum) => Some(Kind::Enum),
        _ => None,
    }
}

/// The language files of a scope, in path order.
pub fn language_files(db: &dyn Db, project: Project, scope: &str) -> Vec<File> {
    let mut found: Vec<File> = project
        .files(db)
        .iter()
        .copied()
        .filter(|&file| {
            let location = jadpo_db::location(db, project, file);
            matches!(location.place, Place::Folder(Folder::Translations))
                && !location.test
                && scope_of(&location.ownership).as_deref() == Some(scope)
                && super::language::is_language_file(db, file)
        })
        .collect();
    found.sort_by(|a, b| a.path(db).cmp(b.path(db)));
    found
}

/// The subjects of a scope.
#[salsa::tracked(returns(ref))]
pub fn subjects<'db>(db: &'db dyn Db, project: Project, scope: Name<'db>) -> Subjects {
    let scope = scope.text(db);
    let mut out = Subjects::default();
    let mut symbols: Vec<&Symbol> = all_symbols(symbols(db, project))
        .filter(|symbol| scope_of(&symbol.ownership).as_deref() == Some(scope.as_str()))
        .filter(|symbol| subject_kind(symbol).is_some())
        .collect();
    symbols.sort_by(|a, b| (&a.path, a.index).cmp(&(&b.path, b.index)));
    for symbol in symbols {
        let Some(kind) = subject_kind(symbol) else {
            continue;
        };
        if kind == Kind::Failure {
            if let Some(subject) =
                failure_subject(db, project, symbol, &out.list, &mut out.problems)
            {
                out.list.push(subject);
            }
            continue;
        }
        let key = to_snake(bare_name(symbol));
        // A name another subject of the scope already took is the checker's
        // naming rules' to report; the first is kept.
        if out.list.iter().any(|seen| seen.key == key) {
            continue;
        }
        let lookup = Lookup::within(db, project, &symbol.ownership);
        let Some(file) = file_at(db, project, Name::new(db, symbol.path.clone())) else {
            continue;
        };
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        let Some(Decl::TypeDecl(decl)) = tree::decl_at(&source, symbol.index) else {
            continue;
        };
        let name_span = decl.name().map_or_else(
            || node_span(&symbol.path, decl.syntax()),
            |name| node_span(&symbol.path, name.syntax()),
        );
        let canonical = lookup.canonical_name(&symbol.name);
        let subject = match kind {
            Kind::Enum => enum_subject(&lookup, &canonical, key, symbol, name_span),
            Kind::Message | Kind::Email | Kind::Failure => {
                let fields = match lookup.def(&canonical) {
                    Some(Def::Fields { fields, .. }) => fields.clone(),
                    _ => Vec::new(),
                };
                let spans = field_spans(&symbol.path, &decl);
                fields_subject(
                    &lookup,
                    kind,
                    key,
                    symbol,
                    name_span,
                    &fields,
                    &spans,
                    &mut out.problems,
                )
            }
        };
        if let Some(mut subject) = subject {
            subject.type_name = known_name(symbol);
            subject.description = crate::graph::text::doc(decl.syntax());
            out.list.push(subject);
        }
    }
    out
}

/// A failure as a subject: keyed by its code, its `client:` fields the
/// placeholders (area 10, "Failures for client apps"). The failure the
/// compiler provides for an entity has no fields and no declaration, so it
/// is pointed at through its entity.
fn failure_subject(
    db: &dyn Db,
    project: Project,
    symbol: &Symbol,
    seen: &[Subject],
    problems: &mut Vec<Diagnostic>,
) -> Option<Subject> {
    let known = known_name(symbol);
    let info = crate::lookups::failure_info(db, project, Name::new(db, known.clone())).as_ref()?;
    // The first failure to use a code keeps it: area 4 reports a code two
    // failures share.
    if seen.iter().any(|other| other.key == info.code) {
        return None;
    }
    let lookup = Lookup::within(db, project, &symbol.ownership);
    let file = file_at(db, project, Name::new(db, symbol.path.clone()))?;
    let source = tree::source(db, file)?;
    let mut description = None;
    let (name_span, spans, fields) = if symbol.provided.is_some() {
        let name_span = symbol
            .provided
            .as_ref()
            .and_then(|entity| symbol_named(db, project, entity))
            .and_then(|found| {
                let decl = tree::decl_at(&source, found.index)?;
                let Decl::EntityDecl(decl) = decl else {
                    return None;
                };
                Some(decl.name().map_or_else(
                    || node_span(&symbol.path, decl.syntax()),
                    |name| node_span(&symbol.path, name.syntax()),
                ))
            })
            .unwrap_or_else(|| Span::new(&symbol.path, 0, 0));
        (name_span, Vec::new(), Vec::new())
    } else {
        let Some(Decl::TypeDecl(decl)) = tree::decl_at(&source, symbol.index) else {
            return None;
        };
        let name_span = decl.name().map_or_else(
            || node_span(&symbol.path, decl.syntax()),
            |name| node_span(&symbol.path, name.syntax()),
        );
        let canonical = lookup.canonical_name(&symbol.name);
        let client = crate::failures::client_fields(db, project, Name::new(db, known.clone()));
        let fields: Vec<FieldDef> = match lookup.def(&canonical) {
            Some(Def::Fields { fields, .. }) => fields
                .iter()
                .filter(|field| client.contains(&field.name))
                .cloned()
                .collect(),
            _ => Vec::new(),
        };
        let spans = failure_field_spans(&symbol.path, &decl);
        description = crate::graph::text::doc(decl.syntax());
        (name_span, spans, fields)
    };
    let at = |name: &str| {
        spans
            .iter()
            .find(|(seen, _)| seen == name)
            .map_or_else(|| name_span.clone(), |(_, span)| span.clone())
    };
    let mut placeholders: Vec<(String, ParamKind)> = Vec::new();
    let mut formats: Vec<(String, ClientFormat)> = Vec::new();
    let mut broken = false;
    for field in &fields {
        match client_class(&lookup, &field.declared) {
            Some((kind, format)) => {
                placeholders.push((field.name.clone(), kind));
                if let Some(format) = format {
                    formats.push((field.name.clone(), format));
                }
            }
            None => {
                broken = true;
                problems.push(unsupported_client(symbol, field, at(&field.name)));
            }
        }
    }
    Some(Subject {
        key: info.code.clone(),
        type_name: known,
        kind: Kind::Failure,
        symbol: symbol.clone(),
        name_span,
        params: Vec::new(),
        placeholders,
        mode: Mode::Plain,
        formats,
        broken,
        builtin: None,
        description,
    })
}

/// The first declaration named `name`.
fn symbol_named(db: &dyn Db, project: Project, name: &str) -> Option<Symbol> {
    symbols(db, project)
        .declarations
        .get(name)
        .and_then(|found| found.first())
        .cloned()
}

/// Where each `client:` field of a failure is written, by name.
fn failure_field_spans(path: &str, decl: &ast::TypeDecl) -> Vec<(String, Span)> {
    tree::members(decl.member_list())
        .into_iter()
        .filter(|(block, _)| tree::name_text(block).as_deref() == Some("client"))
        .flat_map(|(_, member)| tree::members(member.block()))
        .filter_map(|(name, _)| Some((tree::name_text(&name)?, node_span(path, name.syntax()))))
        .collect()
}

/// How a client field is written into a wording, and what the client does
/// to its value first, if it can be written at all.
fn client_class(lookup: &Lookup<'_>, ty: &Ty) -> Option<(ParamKind, Option<ClientFormat>)> {
    if matches!(ty, Ty::Nullable(_)) {
        return None;
    }
    if let Ty::Named(name) = ty
        && matches!(lookup.def(name), Some(Def::Enum { .. }))
    {
        return None;
    }
    Some(match lookup.representation(ty)? {
        // Text on the wire, so written as it is.
        Prim::Text
        | Prim::PresentationText
        | Prim::Uuid
        | Prim::Email
        | Prim::Url
        | Prim::IpAddress => (ParamKind::Text, None),
        Prim::Int => (ParamKind::Number, Some(ClientFormat::Number)),
        Prim::Decimal => (ParamKind::Number, Some(ClientFormat::Decimal)),
        Prim::CalendarDate => (ParamKind::Number, Some(ClientFormat::Date)),
        Prim::Instant | Prim::ZonedTime => (ParamKind::Number, Some(ClientFormat::DateTime)),
        Prim::Duration => (ParamKind::Number, Some(ClientFormat::Duration)),
        _ => return None,
    })
}

fn unsupported_client(symbol: &Symbol, field: &FieldDef, span: Span) -> Diagnostic {
    let ty = field.declared.to_string();
    Diagnostic::new(
        &rule::TRANSLATION_FIELD_TYPE,
        format!(
            "`{}.{}` is typed `{ty}`, which a client can't write into a wording",
            symbol.name, field.name
        ),
    )
    .at(span, "not text, a number, a date, a time or a duration")
    .reason("A failure's `client:` fields are the placeholders of its wording, and the client app fills each one: text as it is, and a number, a date, a time or a duration formatted for its own language. A choice, an object, a list or an optional value has no such format.")
    .next(format!(
        "Make `{}` text, or send the text in a `PresentationText` field",
        field.name
    ))
}

/// Where each field of a type is written, by name.
fn field_spans(path: &str, decl: &ast::TypeDecl) -> Vec<(String, Span)> {
    tree::members(decl.member_list())
        .into_iter()
        .filter_map(|(name, _)| Some((tree::name_text(&name)?, node_span(path, name.syntax()))))
        .collect()
}

/// How a field is written into a wording, if it can be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Class {
    Kind(ParamKind),
    /// An enum, by name.
    Enum(String),
    Unsupported,
}

pub fn classify(lookup: &Lookup<'_>, ty: &Ty) -> Class {
    if matches!(ty, Ty::Nullable(_)) {
        return Class::Unsupported;
    }
    if matches!(ty, Ty::Prim(Prim::PluralCount)) {
        return Class::Kind(ParamKind::Plural);
    }
    if let Ty::Named(name) = ty
        && matches!(lookup.def(name), Some(Def::Enum { .. }))
    {
        return Class::Enum(name.clone());
    }
    match lookup.representation(ty) {
        Some(Prim::Text | Prim::PresentationText) => Class::Kind(ParamKind::Text),
        Some(Prim::Int | Prim::Decimal) => Class::Kind(ParamKind::Number),
        // Written `{price, number, currency}` (area 19, question 16).
        Some(Prim::Money) => Class::Kind(ParamKind::Money),
        // Written `{rate, number, percent}` (area 19, question 16).
        Some(Prim::Percentage) => Class::Kind(ParamKind::Percent),
        _ => Class::Unsupported,
    }
}

fn enum_variants(lookup: &Lookup<'_>, name: &str) -> Vec<VariantInfo> {
    let Some(Def::Enum { variants }) = lookup.def(name) else {
        return Vec::new();
    };
    variants
        .iter()
        .map(|variant| VariantInfo {
            name: variant.name.clone(),
            payload: variant
                .fields
                .iter()
                .flatten()
                .map(|field: &FieldDef| {
                    let kind = match classify(lookup, &field.declared) {
                        Class::Kind(kind @ (ParamKind::Text | ParamKind::Number)) => Some(kind),
                        _ => None,
                    };
                    (field.name.clone(), kind)
                })
                .collect(),
        })
        .collect()
}

fn enum_subject(
    lookup: &Lookup<'_>,
    canonical: &str,
    key: String,
    symbol: &Symbol,
    name_span: Span,
) -> Option<Subject> {
    Some(Subject {
        key,
        type_name: symbol.name.clone(),
        kind: Kind::Enum,
        symbol: symbol.clone(),
        name_span,
        params: vec![ParamSpec {
            name: "value".to_string(),
            kind: ParamKind::Choice,
            enum_type: Some(canonical.to_string()),
        }],
        placeholders: Vec::new(),
        mode: Mode::Variants(enum_variants(lookup, canonical)),
        formats: Vec::new(),
        broken: false,
        builtin: None,
        description: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn fields_subject(
    lookup: &Lookup<'_>,
    kind: Kind,
    key: String,
    symbol: &Symbol,
    name_span: Span,
    fields: &[FieldDef],
    spans: &[(String, Span)],
    problems: &mut Vec<Diagnostic>,
) -> Option<Subject> {
    let at = |name: &str| {
        spans
            .iter()
            .find(|(seen, _)| seen == name)
            .map_or_else(|| name_span.clone(), |(_, span)| span.clone())
    };
    let mut params: Vec<ParamSpec> = Vec::new();
    let mut placeholders: Vec<(String, ParamKind)> = Vec::new();
    let mut selector: Option<(String, Class)> = None;
    let mut broken = false;
    for field in fields {
        let class = classify(lookup, &field.declared);
        match &class {
            Class::Unsupported => {
                // An email's fields feed its templates too; one a subject
                // can't write is simply not a placeholder.
                if kind == Kind::Message {
                    broken = true;
                    problems.push(unsupported(symbol, field, at(&field.name)));
                }
            }
            Class::Kind(
                k @ (ParamKind::Text | ParamKind::Number | ParamKind::Money | ParamKind::Percent),
            ) => {
                params.push(ParamSpec {
                    name: field.name.clone(),
                    kind: *k,
                    enum_type: None,
                });
                placeholders.push((field.name.clone(), *k));
            }
            Class::Kind(_) | Class::Enum(_) => {
                if kind == Kind::Email {
                    continue;
                }
                let (param_kind, enum_type) = match &class {
                    Class::Enum(name) => (ParamKind::Choice, Some(name.clone())),
                    _ => (ParamKind::Plural, None),
                };
                if let Some((first, _)) = &selector {
                    broken = true;
                    problems.push(too_many(
                        symbol,
                        first,
                        &field.name,
                        at(first),
                        at(&field.name),
                    ));
                } else {
                    selector = Some((field.name.clone(), class.clone()));
                }
                params.push(ParamSpec {
                    name: field.name.clone(),
                    kind: param_kind,
                    enum_type,
                });
                if param_kind == ParamKind::Plural {
                    placeholders.push((field.name.clone(), ParamKind::Plural));
                }
            }
        }
    }
    let mode = match &selector {
        Some((_, Class::Enum(name))) => Mode::Variants(enum_variants(lookup, name)),
        Some((_, _)) => Mode::Forms,
        None => Mode::Plain,
    };
    Some(Subject {
        key,
        type_name: symbol.name.clone(),
        kind,
        symbol: symbol.clone(),
        name_span,
        params: if kind == Kind::Email {
            Vec::new()
        } else {
            params
        },
        placeholders,
        mode,
        formats: Vec::new(),
        broken,
        builtin: None,
        description: None,
    })
}

fn unsupported(symbol: &Symbol, field: &FieldDef, span: Span) -> Diagnostic {
    let ty = field.declared.to_string();
    Diagnostic::new(
        &rule::TRANSLATION_FIELD_TYPE,
        format!(
            "`{}.{}` is typed `{ty}`, which a wording can't write",
            symbol.name, field.name
        ),
    )
    .at(span, "not text, a number, money, a percentage or a choice")
    .reason("A message's fields are the placeholders of its wording, and a placeholder is text (`Text`, `PresentationText`), a number (`Int`, `Decimal`), an amount of money (`Money`, written `{field, number, currency}`), a percentage (`Percentage`, written `{field, number, percent}`) or a plural count (`PluralCount`), or a type built on one of them; one `enum` field picks which wording is written. Dates and times are formatted in code, with `text.format`, and passed in as `PresentationText`.")
    .next(format!(
        "Make `{}` a `PresentationText` and format the value where the message is called",
        field.name
    ))
}

fn too_many(symbol: &Symbol, first: &str, second: &str, first_at: Span, at: Span) -> Diagnostic {
    Diagnostic::new(
        &rule::TRANSLATION_TOO_MANY_SELECTORS,
        format!(
            "`{}` has two fields that choose its wording, `{first}` and `{second}`",
            symbol.name
        ),
    )
    .at(at, "a second choice")
    .also(first_at, "the first")
    .reason("An enum field gives one wording for each of its values, and a `PluralCount` field one for each plural form of the language. Both at once would need a wording for every pairing of a value and a form, which a translator would have to fill in.")
    .next(format!(
        "Split `{}` into two messages, one for each choice, and call both",
        symbol.name
    ))
}
