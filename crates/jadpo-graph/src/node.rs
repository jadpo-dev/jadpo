//! Nodes: identity, place, digest and what each kind carries.

use std::fmt;

use jadpo_std::{Builtin, Rules};

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

/// A failure's fields are sent to the caller or kept for the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Section {
    Client,
    Internal,
}

impl Section {
    pub fn word(self) -> &'static str {
        match self {
            Section::Client => "client",
            Section::Internal => "internal",
        }
    }
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
        /// The declared type: `List<Tag>`, `Note.id?`.
        ty: TypeRef,
        optional: bool,
        /// An input field's `default:`, filled in when it is left out.
        default: Option<FieldDefault>,
        section: Option<Section>,
        /// For a field brought in by `include` or `pick`: where it comes from.
        copied_from: Option<Key>,
        /// For a written field that replaces one an `include` or `pick`
        /// brought in: the field it replaces.
        replaces: Option<Key>,
        rules: Rules,
    },
    Variant {
        /// Whether it carries data.
        payload: bool,
    },
    Callable(Signature),
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
            Detail::Callable(signature) => Some(signature),
            _ => None,
        }
    }
}
