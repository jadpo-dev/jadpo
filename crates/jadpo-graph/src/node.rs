//! Nodes: identity, place, digest and what each kind carries.

use std::fmt;

use jadpo_std::{Builtin, Isolation, Rules};

use crate::body::Literal;
use crate::{Key, Kind, TypeRef};

/// Where a node is written: its project-relative file, byte range without
/// surrounding comments, and the lines that range covers (1-based, both
/// ends included). The manifest prints the file and lines; positions never
/// enter a key or a digest.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Place {
    pub file: String,
    pub start: u32,
    pub end: u32,
    pub lines: (u32, u32),
}

/// A BLAKE3 digest of a node's canonical form: its tokens without
/// whitespace, comments or positions, in the order the formatter writes
/// them, so formatting and moving code change neither.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest([u8; 32]);

/// How many hexadecimal digits the manifest prints: 64 bits, enough to tell
/// two versions of a node apart.
pub const DIGEST_DIGITS: usize = 16;

impl Digest {
    /// The digest of `parts`, each length-prefixed so `ab` `c` and `a` `bc`
    /// differ.
    pub fn of<'a>(parts: impl IntoIterator<Item = &'a str>) -> Digest {
        let mut hasher = blake3::Hasher::new();
        for part in parts {
            hasher.update(&(part.len() as u64).to_le_bytes());
            hasher.update(part.as_bytes());
        }
        Digest(*hasher.finalize().as_bytes())
    }

    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The digest as the manifest prints it.
    pub fn short(&self) -> String {
        let mut out = String::with_capacity(DIGEST_DIGITS);
        for byte in &self.0[..DIGEST_DIGITS / 2] {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Digest({})", self.short())
    }
}

/// What a `type` builds on (area 2).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Base {
    Object,
    Enum,
    Input,
    Output,
    Failure,
    Event,
    EmailMessage,
    /// `type X = EmailPartial` (area 16).
    EmailPartial,
    /// `type X = Message` (area 10).
    Message,
    /// `type X = Prompt` (area 21).
    Prompt,
    /// A refinement of a built-in or named type.
    Refines(TypeRef),
}

impl Base {
    /// As written after `=`, naming a refined type's key by `name`.
    pub fn render(&self, name: &dyn Fn(&Key) -> String) -> String {
        match self {
            Base::Object => "Object".to_string(),
            Base::Enum => "Enum".to_string(),
            Base::Input => "Input".to_string(),
            Base::Output => "Output".to_string(),
            Base::Failure => "Failure".to_string(),
            Base::Event => "Event".to_string(),
            Base::EmailMessage => "EmailMessage".to_string(),
            Base::EmailPartial => "EmailPartial".to_string(),
            Base::Message => "Message".to_string(),
            Base::Prompt => "Prompt".to_string(),
            Base::Refines(parent) => parent.render(name),
        }
    }
}

/// A declared failure (area 4).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FailureDetail {
    /// `FailureKind.x` as `x`, when written.
    pub kind: Option<String>,
    /// The public code.
    pub code: String,
    /// The HTTP status its kind sets.
    pub status: Option<u16>,
    pub message: Option<String>,
}

/// A failure's fields are sent to the caller or kept for the log: the
/// failure plans' own, decided once in `jadpo-std`.
pub use jadpo_std::types::Section;

/// How a provider package's field is sent on the wire (area 11,
/// "Decoding"; chunk 7.2c2).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FieldCoding {
    /// One of the built-in encodings, by its name after `Wire.`:
    /// `unix_seconds`.
    Encoding(String),
    /// The package's own `decode:` and `encode:` functions, and the type
    /// the value has on the wire: the one `decode:` takes.
    Functions {
        decode: Key,
        encode: Key,
        wire: TypeRef,
    },
    /// Its `wire_parts:` (chunk 7.2e): its standard type, by name, and
    /// each of its parts with the member of the object holding the field
    /// it is spelled as, in the type's own order.
    Parts {
        standard: String,
        parts: Vec<(String, String)>,
    },
}

/// A callable's signature.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Signature {
    pub params: Vec<(String, TypeRef)>,
    pub result: TypeRef,
    /// The failures its `fails` list names, in order.
    pub fails: Vec<TypeRef>,
}

impl Default for Signature {
    fn default() -> Signature {
        Signature {
            params: Vec::new(),
            result: TypeRef::Builtin(Builtin::Unit),
            fails: Vec::new(),
        }
    }
}

impl Signature {
    /// `(note: Note) fails Refused -> Note`, naming keys by `name`.
    pub fn render(&self, name: &dyn Fn(&Key) -> String) -> String {
        let params: Vec<String> = self
            .params
            .iter()
            .map(|(param, ty)| format!("{param}: {}", ty.render(name)))
            .collect();
        let mut out = format!("({})", params.join(", "));
        if !self.fails.is_empty() {
            let fails: Vec<String> = self.fails.iter().map(|ty| ty.render(name)).collect();
            out.push_str(&format!(" fails {}", fails.join(", ")));
        }
        if self.result != TypeRef::Builtin(Builtin::Unit) {
            out.push_str(&format!(" -> {}", self.result.render(name)));
        }
        out
    }
}

/// What a node of a built kind carries beyond its identity.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Detail {
    #[default]
    None,
    Type {
        base: Base,
        /// Its own rules.
        rules: Rules,
        failure: Option<FailureDetail>,
    },
    Field {
        /// Its place among its owner's fields, from 0, in the order they
        /// are declared (area 2): a field an `include` or `pick` copies
        /// sits where that line is, in the order its source declares it,
        /// and a written field that replaces a copied one takes its place.
        /// A failure's fields count on across its `client` and `internal`
        /// blocks. Values hold their fields in this order.
        order: u32,
        /// The declared type: `List<Tag>`, `Note.id?`.
        ty: TypeRef,
        optional: bool,
        /// An input field's `default:`, filled in when it is left out.
        default: Option<FieldDefault>,
        /// An input field read from a request header: the header's name as
        /// written (area 7).
        header: Option<String>,
        /// An input field read from a file's column of another name, or an
        /// output field written to one: the column's name as written (area
        /// 12, "Batches"; chunks 6.4b and 6.4b2).
        column: Option<String>,
        /// A provider package's field's spelling on the wire, its
        /// `wire_name:` (area 11, decided 2026-10-20).
        wire_name: Option<String>,
        /// How a provider package's field is sent, when its `wire:` or its
        /// `decode:` and `encode:` say (area 11, "Decoding"; chunk 7.2c2).
        coding: Option<FieldCoding>,
        section: Option<Section>,
        /// For a field brought in by `include` or `pick`: where it comes from.
        copied_from: Option<Key>,
        /// For a written field that replaces one an `include` or `pick`
        /// brought in: the field it replaces.
        replaces: Option<Key>,
        rules: Rules,
    },
    Variant {
        /// Its place among its enum's variants, from 0, in the order they
        /// are declared: what a set orders them by (area 2, "Set order").
        order: u32,
        /// Whether it carries data.
        payload: bool,
        /// A provider package's variant's spelling on the wire, its
        /// `wire_name:` (area 11, decided 2026-10-20).
        wire_name: Option<String>,
    },
    /// A rule across an object type's fields (area 2, "Rules across
    /// fields"): its place among its type's rules, which run in that
    /// order, and its condition as written.
    Rule { order: u32, condition: String },
    Callable {
        signature: Signature,
        /// An action's or fixture's: the level its transaction runs at,
        /// read committed when it loads no record, itself or through any
        /// action it calls, otherwise repeatable read (area 6, "Counters
        /// and other one-step updates"). `None` for other callables, which
        /// never begin a transaction of their own.
        isolation: Option<Isolation>,
    },
    /// A constant (area 3): its type and its value as the compiler worked
    /// it out, written as Jadpo writes a literal (`5000`, `0.2000`,
    /// `Verbosity.normal`).
    Constant { ty: TypeRef, value: String },
    /// An entity's store and the table its records live in (area 5).
    Entity { store: String, table: String },
    /// A compound unique rule's or a declared index's fields, in order,
    /// and the name the database knows it by (area 5).
    Columns {
        fields: Vec<String>,
        constraint: String,
    },
    /// A reference (area 5): the entity a field typed `X.id` points at,
    /// what deleting that record does (`restricted`, `cascade`, `cleared`),
    /// and its foreign key's name.
    Reference {
        target: Key,
        on_delete: String,
        constraint: String,
    },
    /// The store an entity lives in (area 5).
    Placement { store: String },
    /// A system table (area 5): its store and table.
    SystemTable { store: String, table: String },
    /// An entity's lifecycle (area 5): which records normal reads see, as
    /// written, and when hidden ones are purged (`after 30d since
    /// deleted_at`).
    Lifecycle {
        visible: Option<String>,
        purge: Option<String>,
    },
    /// A route (area 7): its API, success status, input and output, and
    /// deadline. Its method and full path are its name.
    Route {
        api: String,
        status: u16,
        input: Option<TypeRef>,
        output: Option<TypeRef>,
        deadline_ms: u32,
    },
    /// An API (area 7): the prefix its routes start with.
    Api { prefix: String },
    /// A lifecycle transition (area 5): when it is allowed, as written,
    /// whether that can refuse a visible record (a conflict its callers
    /// map), and the fields it sets.
    Transition {
        when: Option<String>,
        guarded: bool,
        sets: Vec<String>,
    },
    /// One list of an entity's `policy:` (area 9): the effect it grants
    /// (`create`, `read`, `update` or `delete`) and who it names, as
    /// written.
    Grant { effect: String, who: Vec<String> },
    /// A field's own `policy:` (area 9, "Field restrictions"): no client
    /// may read or write it (`FieldAccess.server_only`), or each effect it
    /// narrows, with who, as written.
    FieldRestriction {
        server_only: bool,
        grants: Vec<(String, Vec<String>)>,
    },
    /// Where an entity's records belong (area 9, "Tenancy"): the tenant
    /// entity, and the reference to it (`id` on the tenant itself).
    Tenancy { tenant: Key, via: String },
    /// An action listed under root `policy { actions: }` (area 9,
    /// "Actions"): who may call it, as written.
    ActionException { who: Vec<String> },
    /// A prompt (area 21, "Prompts as files"): its name and the first 8 hex
    /// characters of a SHA-256 over its two files, `answer_question@3fa9c2d1`,
    /// which every call's trace span and log line carry.
    Prompt { version: String },
    /// A connection in `application { connections: }` (area 11): the
    /// capability its driver serves (`database`, `email`, `files`, ...),
    /// or `package` for a provider package, which it names with its
    /// version (`SwiftCouriers.v1`); the domains whose actions may call
    /// it; and the credential scheme its package sends, `bearer`.
    Connection {
        capability: String,
        package: Option<String>,
        domains: Vec<String>,
        credential: Option<String>,
    },
    /// A package the project holds (area 11), keyed by the name its
    /// declarations are written through: its version folder (`v1`),
    /// whether it is a `library` or `provider` package, and whether it is
    /// installed in `packages/` rather than the app's own.
    Package {
        version: String,
        package_kind: String,
        installed: bool,
    },
    /// A provider package's operation (area 11, "How provider calls are
    /// made"): how it is sent, its path as written with its placeholders,
    /// what it takes and gives, and whether it changes anything and may be
    /// sent again.
    Operation {
        method: String,
        path: String,
        input: Option<TypeRef>,
        output: TypeRef,
        changes: bool,
        idempotent: bool,
    },
    /// A provider package's `provide <capability>` (area 11,
    /// "Capabilities"): the capability it fills.
    CapabilityMapping { capability: String },
    /// A host the app can reach (area 11, the network report): its address
    /// as written, or the setting it is read from, or the image of the
    /// service Jadpo runs; the setting and value that choose it, when one
    /// does (`couriers_environment`, `live`); and what it is reached for.
    Host {
        address: Option<String>,
        setting: Option<String>,
        service: Option<String>,
        environment: Option<(String, String)>,
        reached_for: String,
    },
    /// A line of `jadpo.lock` (area 18, "The lock"): the package's version,
    /// where it came from (`registry`, `git`, `path` or `link`), the
    /// repository or folder, a git package's tag and commit, and its
    /// digest.
    LockEntry {
        version: Option<String>,
        source: String,
        location: Option<String>,
        tag: Option<String>,
        commit: Option<String>,
        digest: Option<String>,
    },
    /// A route the runtime answers itself (areas 7, 8 and 11): the API it
    /// is in (`None` for health, which is outside every API), what
    /// generates it (`health`, `openapi`, `exchange`, `passkey`,
    /// `magic_link`, `webhook`), and who may call it (`none`, `caller`,
    /// `service`, `user`, `signature`).
    GeneratedRoute {
        api: Option<String>,
        generated: String,
        authentication: String,
    },
    /// A kind of caller `principal { }` declares (area 8): the entity that
    /// holds it, the field that identifies it, and when its record counts
    /// as active, as written.
    Principal {
        entity: Key,
        subject: String,
        active: Option<String>,
    },
    /// A sign-in method `authentication { }` declares (area 8): whether
    /// the compiler builds it, and a browser session's cookie.
    AuthenticationMethod { built: bool, cookie: Option<String> },
    /// A way `sign_in:` checks a person (area 8, "Checking the person"):
    /// where its routes go, and whether the runtime generates them.
    SignInProvider {
        path: String,
        generates_routes: bool,
    },
    /// A setting in `config { }` (area 10): its type, whether it is
    /// secret, the variable it is read from, and its `default:` in a
    /// variable's spelling.
    ConfigField {
        ty: TypeRef,
        secret: bool,
        variable: String,
        default: Option<String>,
    },
    /// The application's `locales { }` (area 10): its default, the locales
    /// it supports in the order listed, and those whose translations are
    /// in progress, each as `Locale`'s variant (`en_gb`).
    LocaleSet {
        default: String,
        supported: Vec<String>,
        in_progress: Vec<String>,
    },
    /// A rate limit every route is counted against (area 16, "Rate
    /// limits"): how many requests it lets through in its window.
    RateLimit { requests: u32, window_ms: u32 },
    /// A schedule (area 12): how often it runs, in milliseconds.
    Schedule { every_ms: u64 },
    /// A subscriber (area 12): how often a delivery is tried and how long
    /// after its event it is given up, and the event's field naming its
    /// tenant, when the application has one.
    Subscriber {
        attempts: u32,
        within_ms: u64,
        tenant_field: Option<String>,
    },
}

/// What an input field's `default:` fills in when the field is left out
/// (area 2): a literal, or a plain variant of an enum.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FieldDefault {
    Literal(Literal),
    Variant(Key),
}

impl FieldDefault {
    /// As Jadpo writes it: `25`, `none`, `Size.medium`.
    pub fn render(&self, name: &dyn Fn(&Key) -> String) -> String {
        match self {
            FieldDefault::Literal(literal) => literal.written(),
            FieldDefault::Variant(key) => name(key),
        }
    }
}

/// One node.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Node {
    pub key: Key,
    pub kind: Kind,
    /// Its name now, as written: differs from the key's only after a rename.
    pub name: String,
    pub place: Option<Place>,
    pub digest: Option<Digest>,
    /// The domain that owns it; `None` for `shared/`, the root and loose
    /// files.
    pub domain: Option<String>,
    /// What it belongs to: a field's type, a variant's enum, a library
    /// function's library.
    pub parent: Option<Key>,
    /// Visible to other domains (area 1).
    pub public: bool,
    /// Its `///` description.
    pub doc: Option<String>,
    pub detail: Detail,
}

impl Node {
    pub fn new(key: Key, kind: Kind, name: impl Into<String>) -> Node {
        Node {
            key,
            kind,
            name: name.into(),
            place: None,
            digest: None,
            domain: None,
            parent: None,
            public: false,
            doc: None,
            detail: Detail::None,
        }
    }

    pub fn signature(&self) -> Option<&Signature> {
        match &self.detail {
            Detail::Callable { signature, .. } => Some(signature),
            _ => None,
        }
    }

    /// An action's or fixture's isolation level (area 6).
    pub fn isolation(&self) -> Option<Isolation> {
        match &self.detail {
            Detail::Callable { isolation, .. } => *isolation,
            _ => None,
        }
    }
}
