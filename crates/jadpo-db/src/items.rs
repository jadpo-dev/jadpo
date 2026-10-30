//! Item trees (docs/compiler-runtime.md §2.2): each file summarised into its
//! declarations and their signatures, without bodies and without positions,
//! so an edit inside a body, or one that only moves code, leaves the item
//! tree equal and stops invalidation there.

use jadpo_syntax::SyntaxNode;
use jadpo_syntax::ast::{self, AstNode, Decl, MemberItem};

use crate::{Db, File, Location, Place, parse};

/// What a declaration is.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ItemKind {
    Type(Base),
    Function,
    /// `constant name: Type = value` (area 3): its type is the signature's
    /// result.
    Constant,
    Action,
    Query,
    Fixture,
    Entity,
    Library,
    Routes,
    Schedule,
    Subscriber,
    Test,
    Seed,
    Package,
    Resource,
    Provide,
    /// `application { }`, `config { }` and the other unnamed singletons;
    /// the item's name is the keyword.
    Singleton,
}

impl ItemKind {
    /// The kind in the reader's words, for messages.
    pub fn word(&self) -> &'static str {
        match self {
            ItemKind::Type(Base::Failure) => "failure",
            ItemKind::Type(Base::Event) => "event",
            ItemKind::Type(Base::EmailMessage) => "email",
            ItemKind::Type(Base::EmailPartial) => "email partial",
            ItemKind::Type(Base::Message) => "message",
            ItemKind::Type(Base::Prompt) => "prompt",
            ItemKind::Type(_) => "type",
            ItemKind::Function => "function",
            ItemKind::Constant => "constant",
            ItemKind::Action => "action",
            ItemKind::Query => "query",
            ItemKind::Fixture => "fixture",
            ItemKind::Entity => "entity",
            ItemKind::Library => "library",
            ItemKind::Routes => "route group",
            ItemKind::Schedule => "schedule",
            ItemKind::Subscriber => "subscriber",
            ItemKind::Test => "test",
            ItemKind::Seed => "seed",
            ItemKind::Package => "package",
            ItemKind::Resource => "resource",
            ItemKind::Provide => "provider mapping",
            ItemKind::Singleton => "setting block",
        }
    }

    /// The kind with its article: `a type`, `an action`.
    pub fn with_a(&self) -> String {
        let word = self.word();
        if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
            format!("an {word}")
        } else {
            format!("a {word}")
        }
    }

    /// Whether a value of this kind is written in `UpperCamelCase`.
    pub fn upper_camel(&self) -> bool {
        matches!(
            self,
            ItemKind::Type(_) | ItemKind::Entity | ItemKind::Routes | ItemKind::Package
        )
    }

    pub fn is_failure(&self) -> bool {
        matches!(self, ItemKind::Type(Base::Failure))
    }
}

/// What a `type` declaration builds on.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Base {
    Object,
    Enum,
    Input,
    Output,
    Failure,
    Event,
    EmailMessage,
    /// `type X = EmailPartial { ... }` (area 16, "Email"): a piece of
    /// an email's templates, its `.html` and `.txt` beside it, included
    /// with its fields by name.
    EmailPartial,
    /// `type X = Message { ... }` (area 10, "Translations as messages"):
    /// text for people, its fields the wording's placeholders.
    Message,
    /// `type X = Prompt { ... }` (area 21, "Prompts as files"): what a
    /// model is asked, its fields the placeholders of its `.prompt` file.
    Prompt,
    /// A built-in representation or another named type, as written.
    Named(String),
}

impl Base {
    /// The base a `type` declaration names, as written after `=`.
    pub fn from_written(text: &str) -> Base {
        match text {
            "Object" => Base::Object,
            "Enum" => Base::Enum,
            "Input" => Base::Input,
            "Output" => Base::Output,
            "Failure" => Base::Failure,
            "Event" => Base::Event,
            "EmailMessage" => Base::EmailMessage,
            "EmailPartial" => Base::EmailPartial,
            "Message" => Base::Message,
            "Prompt" => Base::Prompt,
            other => Base::Named(other.to_string()),
        }
    }

    /// Whether the type's block holds fields, whose values are types.
    pub fn has_fields(&self) -> bool {
        matches!(
            self,
            Base::Object
                | Base::Input
                | Base::Output
                | Base::Event
                | Base::EmailMessage
                | Base::EmailPartial
                | Base::Message
                | Base::Prompt
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Param {
    pub name: String,
    /// The type as written, without spaces or comments.
    pub ty: String,
}

/// A callable's signature as written.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Signature {
    pub params: Vec<Param>,
    pub fails: Vec<String>,
    pub result: Option<String>,
}

/// One declaration.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Item {
    pub kind: ItemKind,
    pub name: String,
    pub public: bool,
    /// Its position among the file's declarations, counting from 0.
    pub index: u32,
    pub signature: Signature,
    /// A library's functions and constants, in source order.
    pub functions: Vec<Item>,
    /// A route group's prefix: its fixed segments, placeholders left out.
    pub prefix: Option<Vec<String>>,
    /// For `application { }`: the connection names it declares.
    pub connections: Vec<String>,
    /// For `application { }`: the packages its connections name.
    pub packages: Vec<String>,
    /// For `application { }`: each connection to a package, with the
    /// package as its `package:` writes it, `AcmeCouriers.v1`.
    pub connected: Vec<(String, String)>,
    /// For an entity: the transitions its `lifecycle:` names, each an
    /// operation beside `get`, `create`, `update` and `delete` (area 6).
    pub transitions: Vec<String>,
}

/// A file's declarations, in source order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ItemTree {
    pub items: Vec<Item>,
}

/// The item tree of `file`.
#[salsa::tracked(returns(ref))]
pub fn item_tree(db: &dyn Db, file: File) -> ItemTree {
    let Some(source) = ast::SourceFile::cast(parse(db, file).syntax()) else {
        return ItemTree::default();
    };
    let mut items = items_of(&source);
    // `seed { }` is named by its file (area 5, "Seeds").
    for item in &mut items {
        if item.kind == ItemKind::Seed && item.name.is_empty() {
            item.name = file_stem(file.path(db));
        }
    }
    ItemTree { items }
}

/// A seed's name: the one it is written with, or for `seed { }` its file
/// name without `.jadpo` (area 5, "Seeds").
pub fn seed_name(decl: &ast::SeedDecl, path: &str) -> String {
    name_of(decl.name()).unwrap_or_else(|| file_stem(path))
}

/// `database/seeds/first_staff.jadpo`'s `first_staff`.
fn file_stem(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.strip_suffix(".jadpo").unwrap_or(file).to_string()
}

/// A file's declarations, in source order: what [`item_tree`] holds, for a
/// caller with a syntax tree and no database, such as `jadpo fmt`.
pub fn items_of(source: &ast::SourceFile) -> Vec<Item> {
    source
        .decls()
        .enumerate()
        .filter_map(|(index, decl)| lower(&decl, index as u32))
        .collect()
}

/// The declaration a file is named after and exists for (area 1, "Files
/// and names"): in a fixed folder, the one of the folder's kind whose name
/// gives the file name, or the first of that kind when the file is
/// misnamed; in `database/seeds/`, the seed. Everything else in the file
/// supports it. Test files, root files, loose files and files the layout
/// has no place for have none.
pub fn primary<'a>(location: &Location, items: &'a [Item]) -> Option<&'a Item> {
    if location.test {
        return None;
    }
    let main: Vec<&Item> = match location.place {
        Place::Folder(folder) => items
            .iter()
            .filter(|item| folder.holds(&item.kind))
            .collect(),
        Place::Seeds => items
            .iter()
            .filter(|item| item.kind == ItemKind::Seed)
            .collect(),
        _ => return None,
    };
    main.iter()
        .find(|item| crate::places::to_snake(&item.name) == location.stem)
        .or(main.first())
        .copied()
}

/// A node's tokens without trivia: `List<Tag>`, `Todo.id`, `Name?`.
pub(crate) fn written(node: &SyntaxNode) -> String {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .map(|token| token.text().to_string())
        .collect()
}

fn name_of(name: Option<ast::Name>) -> Option<String> {
    Some(name?.ident_token()?.text().to_string())
}

fn signature(
    params: Option<ast::ParamList>,
    fails: Option<ast::FailsClause>,
    result: Option<ast::RetType>,
) -> Signature {
    Signature {
        params: params
            .into_iter()
            .flat_map(|list| list.params())
            .filter_map(|param| {
                Some(Param {
                    name: name_of(param.name())?,
                    ty: param
                        .ty()
                        .map_or_else(String::new, |ty| written(ty.syntax())),
                })
            })
            .collect(),
        fails: fails
            .into_iter()
            .flat_map(|clause| clause.types())
            .map(|ty| written(ty.syntax()))
            .collect(),
        result: result
            .and_then(|ret| ret.ty())
            .map(|ty| written(ty.syntax())),
    }
}

fn item(kind: ItemKind, name: String, public: bool, index: u32) -> Item {
    Item {
        kind,
        name,
        public,
        index,
        signature: Signature::default(),
        functions: Vec::new(),
        prefix: None,
        connections: Vec::new(),
        packages: Vec::new(),
        connected: Vec::new(),
        transitions: Vec::new(),
    }
}

fn lower(decl: &Decl, index: u32) -> Option<Item> {
    let lowered = match decl {
        Decl::TypeDecl(decl) => {
            let base = decl.base().map_or(Base::Named(String::new()), |base| {
                Base::from_written(&written(base.syntax()))
            });
            item(
                ItemKind::Type(base),
                name_of(decl.name())?,
                decl.visibility().is_some(),
                index,
            )
        }
        Decl::FunctionDecl(decl) => Item {
            signature: signature(decl.param_list(), decl.fails_clause(), decl.ret_type()),
            ..item(
                ItemKind::Function,
                name_of(decl.name())?,
                decl.visibility().is_some(),
                index,
            )
        },
        Decl::ConstantDecl(decl) => Item {
            signature: Signature {
                result: decl.ty().map(|ty| written(ty.syntax())),
                ..Signature::default()
            },
            ..item(
                ItemKind::Constant,
                name_of(decl.name())?,
                decl.visibility().is_some(),
                index,
            )
        },
        Decl::ActionDecl(decl) => Item {
            signature: signature(decl.param_list(), decl.fails_clause(), decl.ret_type()),
            ..item(
                ItemKind::Action,
                name_of(decl.name())?,
                decl.visibility().is_some(),
                index,
            )
        },
        Decl::QueryDecl(decl) => Item {
            signature: signature(decl.param_list(), decl.fails_clause(), decl.ret_type()),
            ..item(
                ItemKind::Query,
                name_of(decl.name())?,
                decl.visibility().is_some(),
                index,
            )
        },
        Decl::FixtureDecl(decl) => Item {
            signature: signature(decl.param_list(), None, decl.ret_type()),
            ..item(
                ItemKind::Fixture,
                name_of(decl.name())?,
                decl.visibility().is_some(),
                index,
            )
        },
        Decl::EntityDecl(decl) => Item {
            transitions: members(decl.member_list())
                .filter(|(key, _)| key == "lifecycle")
                .flat_map(|(_, lifecycle)| members(lifecycle.block()))
                .filter(|(key, _)| key == "transitions")
                .flat_map(|(_, transitions)| members(transitions.block()))
                .map(|(name, _)| name)
                .collect(),
            ..item(
                ItemKind::Entity,
                name_of(decl.name())?,
                decl.visibility().is_some(),
                index,
            )
        },
        Decl::LibraryDecl(decl) => {
            let functions = decl
                .member_list()
                .into_iter()
                .flat_map(|list| list.member_items())
                .filter_map(|member| match member {
                    MemberItem::Decl(inner @ (Decl::FunctionDecl(_) | Decl::ConstantDecl(_))) => {
                        lower(&inner, 0)
                    }
                    _ => None,
                })
                .enumerate()
                .map(|(i, function)| Item {
                    index: i as u32,
                    ..function
                })
                .collect();
            Item {
                functions,
                ..item(
                    ItemKind::Library,
                    name_of(decl.name())?,
                    decl.visibility().is_some(),
                    index,
                )
            }
        }
        Decl::RoutesDecl(decl) => Item {
            prefix: route_prefix(decl.member_list()),
            ..item(ItemKind::Routes, name_of(decl.name())?, false, index)
        },
        Decl::ScheduleDecl(decl) => item(ItemKind::Schedule, name_of(decl.name())?, false, index),
        Decl::SubscriberDecl(decl) => {
            item(ItemKind::Subscriber, name_of(decl.name())?, false, index)
        }
        Decl::TestDecl(decl) => {
            let name = decl.string_token()?.text().trim_matches('"').to_string();
            item(ItemKind::Test, name, false, index)
        }
        // `seed { }` has no name of its own; [`item_tree`] gives it its
        // file's.
        Decl::SeedDecl(decl) => item(
            ItemKind::Seed,
            name_of(decl.name()).unwrap_or_default(),
            false,
            index,
        ),
        Decl::PackageDecl(decl) => item(
            ItemKind::Package,
            written(decl.path()?.syntax()),
            false,
            index,
        ),
        Decl::ResourceDecl(decl) => item(
            ItemKind::Resource,
            name_of(decl.name())?,
            decl.visibility().is_some(),
            index,
        ),
        Decl::ProvideDecl(decl) => item(ItemKind::Provide, name_of(decl.name())?, false, index),
        Decl::SingletonDecl(decl) => {
            let keyword = decl.keyword()?.text().to_string();
            let (connections, packages, connected) = connections(decl.member_list());
            Item {
                connections,
                packages,
                connected,
                ..item(ItemKind::Singleton, keyword, false, index)
            }
        }
    };
    Some(lowered)
}

/// The members of a block, by their single-name keys.
fn members(list: Option<ast::MemberList>) -> impl Iterator<Item = (String, ast::Member)> {
    list.into_iter()
        .flat_map(|list| list.member_items())
        .filter_map(|item| match item {
            MemberItem::Member(member) => {
                let mut names = member.member_key()?.names();
                let first = names.next()?;
                if names.next().is_some() {
                    return None;
                }
                Some((name_of(Some(first))?, member))
            }
            _ => None,
        })
}

/// A route group's `prefix:` as fixed segments.
fn route_prefix(list: Option<ast::MemberList>) -> Option<Vec<String>> {
    let (_, member) = members(list).find(|(key, _)| key == "prefix")?;
    let ast::Expr::RoutePathExpr(path) = member.value()? else {
        return None;
    };
    Some(
        path.route_path()?
            .route_segments()
            .filter_map(|segment| Some(segment.path_segment_token()?.text().to_string()))
            .collect(),
    )
}

/// The connections an `application { }` block declares (`connections:`,
/// and the `stores:` and `providers:` it replaces), and the packages they
/// name.
#[allow(clippy::type_complexity)]
fn connections(list: Option<ast::MemberList>) -> (Vec<String>, Vec<String>, Vec<(String, String)>) {
    let mut names = Vec::new();
    let mut packages = Vec::new();
    let mut connected = Vec::new();
    for (key, member) in members(list) {
        if !matches!(key.as_str(), "connections" | "stores" | "providers") {
            continue;
        }
        for (name, entry) in members(member.block()) {
            for (setting, value) in members(entry.block()) {
                if setting == "package"
                    && let Some(value) = value.value()
                {
                    let text = written(value.syntax());
                    let ownership = text.split('.').next().unwrap_or_default();
                    packages.push(ownership.to_string());
                    connected.push((name.clone(), text));
                }
            }
            names.push(name);
        }
    }
    (names, packages, connected)
}
