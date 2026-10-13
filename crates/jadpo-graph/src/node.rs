//! Nodes: identity, place, digest and what each kind carries.

use std::fmt;

use crate::{Key, Kind};

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
    /// A refinement of a built-in or named type, as written.
    Refines(String),
}

impl Base {
    pub fn word(&self) -> &str {
        match self {
            Base::Object => "Object",
            Base::Enum => "Enum",
            Base::Input => "Input",
            Base::Output => "Output",
            Base::Failure => "Failure",
            Base::Event => "Event",
            Base::EmailMessage => "EmailMessage",
            Base::Refines(parent) => parent,
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

/// A callable's signature, its types written canonically.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Signature {
    pub params: Vec<(String, String)>,
    pub result: String,
    /// The failures its `fails` list names, as written and in order.
    pub fails: Vec<String>,
}

impl Signature {
    /// `(note: Note) fails Refused -> Note`.
    pub fn render(&self) -> String {
        let params: Vec<String> = self
            .params
            .iter()
            .map(|(name, ty)| format!("{name}: {ty}"))
            .collect();
        let mut out = format!("({})", params.join(", "));
        if !self.fails.is_empty() {
            out.push_str(&format!(" fails {}", self.fails.join(", ")));
        }
        if self.result != "Unit" {
            out.push_str(&format!(" -> {}", self.result));
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
        /// Its own rules, each as `name` and canonical value.
        rules: Vec<(String, String)>,
        failure: Option<FailureDetail>,
    },
    Field {
        /// The declared type, canonically: `List<Tag>`, `Note.id?`.
        ty: String,
        optional: bool,
        default: bool,
        section: Option<Section>,
        /// For a field brought in by `include` or `pick`: where it comes from.
        copied_from: Option<Key>,
        rules: Vec<(String, String)>,
    },
    Variant {
        /// Whether it carries data.
        payload: bool,
    },
    Callable(Signature),
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
