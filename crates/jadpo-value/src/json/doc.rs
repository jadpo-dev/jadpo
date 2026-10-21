//! A JSON document read in place (chunk 8.1p; docs/compiler-runtime.md
//! §2.5, "A typed per-route entry"): what compiled code's generated
//! readers walk instead of a `serde_json` value. [`Doc::parse`] reads the
//! bytes once into a flat list of nodes over them, a string or a number a
//! span of the text, so nothing is copied but a string holding an escape
//! when it is read; it accepts exactly the documents [`super::parse`]
//! accepts (RFC 8259 as `serde_json` reads it, no member named twice, 128
//! levels deep, every number finite), which a seeded comparison of the two
//! holds it to.
//!
//! An [`Item`] is one value wherever it comes from: a node of a document,
//! a `serde_json` value, or text a query string or a header gave, read as
//! a JSON string, or several, read as an array of them. [`Members`] is an
//! object's members, a document's, a query string's or none, with the
//! fields read from headers in place of any member of their name.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use serde_json::Value as Json;

use super::Numbers;
use super::document::number_of;

/// How deep arrays and objects may nest, as [`super::parse`] allows.
const DEPTH: usize = 128;

/// Up to this many members an object's names are compared one by one for
/// a repeat; past it, by a set, so no object is read in quadratic time.
const COMPARED: usize = 16;

/// A JSON document, read in place.
#[derive(Debug)]
pub struct Doc<'t> {
    text: &'t str,
    nodes: Vec<Node>,
}

/// One value of a document, in the order the text writes them: a
/// container's node first, then each of its values (an object's as its
/// name's `String` node and then the value).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Node {
    Null,
    Bool(bool),
    /// Its text's bytes, and whether it has neither fraction nor exponent.
    Number {
        start: u32,
        end: u32,
        whole: bool,
    },
    /// Its text's bytes, quotes included, and whether it holds an escape.
    String {
        start: u32,
        end: u32,
        escaped: bool,
    },
    /// How many items it has, and the node after its last.
    Array {
        count: u32,
        next: u32,
    },
    /// How many members it has, and the node after its last.
    Object {
        count: u32,
        next: u32,
    },
}

impl<'t> Doc<'t> {
    /// The document `bytes` hold; `None` when they aren't one JSON
    /// document in UTF-8, or an object names a member twice, exactly as
    /// [`super::parse`] refuses them.
    pub fn parse(bytes: &'t [u8]) -> Option<Doc<'t>> {
        let text = std::str::from_utf8(bytes).ok()?;
        let mut reader = Reader {
            text,
            at: 0,
            // About one node for every few bytes of a request's body, so
            // the list is made once.
            nodes: Vec::with_capacity(text.len() / 6 + 4),
        };
        reader.space();
        reader.value(0)?;
        reader.space();
        (reader.at == text.len()).then_some(Doc {
            text,
            nodes: reader.nodes,
        })
    }

    /// Its top value.
    pub fn root(&self) -> Item<'_> {
        Item::Node(self, 0)
    }

    fn node(&self, at: u32) -> Option<Node> {
        self.nodes.get(at as usize).copied()
    }

    /// The node after node `at` and everything inside it.
    fn after(&self, at: u32) -> u32 {
        after(&self.nodes, at)
    }

    fn span(&self, start: u32, end: u32) -> &'t str {
        self.text
            .get(start as usize..end as usize)
            .unwrap_or_default()
    }

    /// A string node's text: borrowed unless it holds an escape.
    fn string(&self, start: u32, end: u32, escaped: bool) -> Cow<'t, str> {
        if escaped {
            Cow::Owned(serde_json::from_str::<String>(self.span(start, end)).unwrap_or_default())
        } else {
            Cow::Borrowed(self.span(start + 1, end.saturating_sub(1)))
        }
    }

    /// Node `at` as the `serde_json` value [`super::parse`] gives for it.
    fn json(&self, at: u32) -> Json {
        match self.node(at) {
            None | Some(Node::Null) => Json::Null,
            Some(Node::Bool(flag)) => Json::Bool(flag),
            Some(Node::Number { start, end, whole }) => {
                number_of(self.span(start, end), whole).map_or(Json::Null, Json::Number)
            }
            Some(Node::String {
                start,
                end,
                escaped,
            }) => Json::String(self.string(start, end, escaped).into_owned()),
            Some(Node::Array { count, .. }) => {
                let mut items = Vec::with_capacity(count as usize);
                let mut item = at + 1;
                for _ in 0..count {
                    items.push(self.json(item));
                    item = self.after(item);
                }
                Json::Array(items)
            }
            Some(Node::Object { count, .. }) => {
                let mut members = serde_json::Map::new();
                let mut name = at + 1;
                for _ in 0..count {
                    let value = name + 1;
                    if let Some(Node::String {
                        start,
                        end,
                        escaped,
                    }) = self.node(name)
                    {
                        members.insert(
                            self.string(start, end, escaped).into_owned(),
                            self.json(value),
                        );
                    }
                    name = self.after(value);
                }
                Json::Object(members)
            }
        }
    }

    /// Each number's text as written, by where it stands from the top, as
    /// [`super::parse`] gives them.
    pub fn numbers(&self) -> Numbers {
        let mut out = HashMap::new();
        let mut path = Vec::new();
        self.numbers_in(0, &mut path, &mut out);
        Numbers::from_map(out)
    }

    fn numbers_in(&self, at: u32, path: &mut Vec<String>, out: &mut HashMap<Vec<String>, String>) {
        match self.node(at) {
            Some(Node::Number { start, end, .. }) => {
                out.insert(path.clone(), self.span(start, end).to_string());
            }
            Some(Node::Array { count, .. }) => {
                let mut item = at + 1;
                for n in 0..count {
                    path.push(n.to_string());
                    self.numbers_in(item, path, out);
                    path.pop();
                    item = self.after(item);
                }
            }
            Some(Node::Object { count, .. }) => {
                let mut name = at + 1;
                for _ in 0..count {
                    let value = name + 1;
                    if let Some(Node::String {
                        start,
                        end,
                        escaped,
                    }) = self.node(name)
                    {
                        path.push(self.string(start, end, escaped).into_owned());
                        self.numbers_in(value, path, out);
                        path.pop();
                    }
                    name = self.after(value);
                }
            }
            _ => {}
        }
    }
}

/// The node after node `at` of `nodes` and everything inside it: once a
/// container is read, the node it says follows it.
fn after(nodes: &[Node], at: u32) -> u32 {
    match nodes.get(at as usize) {
        Some(Node::Array { next, .. } | Node::Object { next, .. }) => *next,
        _ => at + 1,
    }
}

/// What reads a document: [`super::parse`]'s grammar, writing nodes.
struct Reader<'t> {
    text: &'t str,
    at: usize,
    nodes: Vec<Node>,
}

impl<'t> Reader<'t> {
    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn push(&mut self, node: Node) -> Option<()> {
        self.nodes.push(node);
        Some(())
    }

    fn word(&mut self, word: &str, node: Node) -> Option<()> {
        if !self.text.get(self.at..)?.starts_with(word) {
            return None;
        }
        self.at += word.len();
        self.push(node)
    }

    fn value(&mut self, depth: usize) -> Option<()> {
        match self.peek()? {
            b'{' => self.object(depth + 1),
            b'[' => self.array(depth + 1),
            b'"' => {
                let node = self.string()?;
                self.push(node)
            }
            b't' => self.word("true", Node::Bool(true)),
            b'f' => self.word("false", Node::Bool(false)),
            b'n' => self.word("null", Node::Null),
            b'-' | b'0'..=b'9' => self.number(),
            _ => None,
        }
    }

    /// The text of a name, as [`Doc::string`] reads it.
    fn name(&self, (start, end, escaped): (u32, u32, bool)) -> Option<Cow<'t, str>> {
        let span = self.text.get(start as usize..end as usize)?;
        if escaped {
            serde_json::from_str::<String>(span).ok().map(Cow::Owned)
        } else {
            span.get(1..span.len().saturating_sub(1)).map(Cow::Borrowed)
        }
    }

    fn object(&mut self, depth: usize) -> Option<()> {
        if depth > DEPTH {
            return None;
        }
        self.at += 1;
        let at = u32::try_from(self.nodes.len()).ok()?;
        self.push(Node::Object { count: 0, next: 0 })?;
        // Past `COMPARED` names, each one's text, for a repeat.
        let mut hashed: Option<HashSet<Cow<'t, str>>> = None;
        let mut count = 0u32;
        self.space();
        if self.peek()? == b'}' {
            self.at += 1;
        } else {
            loop {
                self.space();
                if self.peek()? != b'"' {
                    return None;
                }
                let node = self.string()?;
                let Node::String {
                    start,
                    end,
                    escaped,
                } = node
                else {
                    return None;
                };
                let name = (start, end, escaped);
                match &mut hashed {
                    Some(seen) => {
                        if !seen.insert(self.name(name)?) {
                            return None;
                        }
                    }
                    None => {
                        // The names read so far are this object's own
                        // key nodes, each before its value.
                        let given = self.name(name)?;
                        let mut earlier = Vec::new();
                        let mut key = at + 1;
                        for _ in 0..count {
                            if let Some(Node::String {
                                start,
                                end,
                                escaped,
                            }) = self.nodes.get(key as usize).copied()
                            {
                                let read = self.name((start, end, escaped))?;
                                if read == given {
                                    return None;
                                }
                                if count as usize >= COMPARED {
                                    earlier.push(read);
                                }
                            }
                            key = after(&self.nodes, key + 1);
                        }
                        if count as usize >= COMPARED {
                            let mut seen: HashSet<Cow<'t, str>> = earlier.into_iter().collect();
                            seen.insert(given);
                            hashed = Some(seen);
                        }
                    }
                }
                self.push(node)?;
                self.space();
                if self.peek()? != b':' {
                    return None;
                }
                self.at += 1;
                self.space();
                self.value(depth)?;
                count += 1;
                self.space();
                match self.peek()? {
                    b',' => self.at += 1,
                    b'}' => {
                        self.at += 1;
                        break;
                    }
                    _ => return None,
                }
            }
        }
        let next = u32::try_from(self.nodes.len()).ok()?;
        *self.nodes.get_mut(at as usize)? = Node::Object { count, next };
        Some(())
    }

    fn array(&mut self, depth: usize) -> Option<()> {
        if depth > DEPTH {
            return None;
        }
        self.at += 1;
        let at = self.nodes.len();
        self.push(Node::Array { count: 0, next: 0 })?;
        let mut count = 0u32;
        self.space();
        if self.peek()? == b']' {
            self.at += 1;
        } else {
            loop {
                self.space();
                self.value(depth)?;
                count += 1;
                self.space();
                match self.peek()? {
                    b',' => self.at += 1,
                    b']' => {
                        self.at += 1;
                        break;
                    }
                    _ => return None,
                }
            }
        }
        let next = u32::try_from(self.nodes.len()).ok()?;
        *self.nodes.get_mut(at)? = Node::Array { count, next };
        Some(())
    }

    /// A string, scanned as [`super::parse`] scans one: its escapes read
    /// by `serde_json`, and without one, refused for a control character
    /// as `serde_json` refuses it.
    fn string(&mut self) -> Option<Node> {
        let start = self.at;
        self.at += 1;
        let mut escaped = false;
        loop {
            match self.peek()? {
                b'"' => break,
                b'\\' => {
                    escaped = true;
                    self.at += 2;
                }
                _ => self.at += 1,
            }
        }
        self.at += 1;
        let span = self.text.get(start..self.at)?;
        if escaped {
            serde_json::from_str::<String>(span).ok()?;
        } else if span.bytes().any(|byte| byte < 0x20) {
            return None;
        }
        Some(Node::String {
            start: u32::try_from(start).ok()?,
            end: u32::try_from(self.at).ok()?,
            escaped,
        })
    }

    /// A number, by [`super::parse`]'s grammar, refused where `serde_json`
    /// can't keep it.
    fn number(&mut self) -> Option<()> {
        let bytes = self.text.as_bytes();
        let start = self.at;
        let digits = |at: &mut usize| {
            let from = *at;
            while bytes.get(*at).is_some_and(u8::is_ascii_digit) {
                *at += 1;
            }
            *at > from
        };
        let mut at = start;
        if bytes.get(at) == Some(&b'-') {
            at += 1;
        }
        if bytes.get(at) == Some(&b'0') {
            at += 1;
        } else if !digits(&mut at) {
            return None;
        }
        let mut whole = true;
        if bytes.get(at) == Some(&b'.') {
            at += 1;
            whole = false;
            if !digits(&mut at) {
                return None;
            }
        }
        if matches!(bytes.get(at), Some(b'e' | b'E')) {
            at += 1;
            whole = false;
            if matches!(bytes.get(at), Some(b'+' | b'-')) {
                at += 1;
            }
            if !digits(&mut at) {
                return None;
            }
        }
        self.at = at;
        number_of(self.text.get(start..at)?, whole)?;
        self.push(Node::Number {
            start: u32::try_from(start).ok()?,
            end: u32::try_from(at).ok()?,
            whole,
        })
    }
}

/// One JSON value, wherever it comes from.
#[derive(Clone, Copy, Debug)]
pub enum Item<'a> {
    /// A node of a document read in place.
    Node(&'a Doc<'a>, u32),
    /// A `serde_json` value.
    Json(&'a Json),
    /// Text a query string or a header gave: a JSON string.
    Text(&'a str),
    /// A query string's values for one key: a JSON array of strings.
    Texts(&'a [String]),
}

/// What kind of JSON value an [`Item`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Null,
    Bool,
    Number,
    String,
    Array,
    Object,
}

impl<'a> Item<'a> {
    pub fn kind(self) -> Kind {
        match self {
            Item::Node(doc, at) => match doc.node(at) {
                None | Some(Node::Null) => Kind::Null,
                Some(Node::Bool(_)) => Kind::Bool,
                Some(Node::Number { .. }) => Kind::Number,
                Some(Node::String { .. }) => Kind::String,
                Some(Node::Array { .. }) => Kind::Array,
                Some(Node::Object { .. }) => Kind::Object,
            },
            Item::Json(json) => match json {
                Json::Null => Kind::Null,
                Json::Bool(_) => Kind::Bool,
                Json::Number(_) => Kind::Number,
                Json::String(_) => Kind::String,
                Json::Array(_) => Kind::Array,
                Json::Object(_) => Kind::Object,
            },
            Item::Text(_) => Kind::String,
            Item::Texts(_) => Kind::Array,
        }
    }

    pub fn is_null(self) -> bool {
        self.kind() == Kind::Null
    }

    /// A `true` or `false`.
    pub fn as_bool(self) -> Option<bool> {
        match self {
            Item::Node(doc, at) => match doc.node(at)? {
                Node::Bool(flag) => Some(flag),
                _ => None,
            },
            Item::Json(json) => json.as_bool(),
            Item::Text(_) | Item::Texts(_) => None,
        }
    }

    /// A string's text.
    pub fn text(self) -> Option<Cow<'a, str>> {
        match self {
            Item::Node(doc, at) => match doc.node(at)? {
                Node::String {
                    start,
                    end,
                    escaped,
                } => Some(doc.string(start, end, escaped)),
                _ => None,
            },
            Item::Json(Json::String(text)) => Some(Cow::Borrowed(text)),
            Item::Json(_) | Item::Texts(_) => None,
            Item::Text(text) => Some(Cow::Borrowed(text)),
        }
    }

    /// A number as `serde_json`'s `as_i64` reads it: a whole number
    /// written without fraction or exponent that fits, never `-0`, which
    /// `serde_json` keeps as a double.
    pub fn as_i64(self) -> Option<i64> {
        match self {
            Item::Node(doc, at) => match doc.node(at)? {
                Node::Number { start, end, whole } => {
                    let written = doc.span(start, end);
                    if !whole || written == "-0" {
                        return None;
                    }
                    written.parse().ok()
                }
                _ => None,
            },
            Item::Json(json) => json.as_i64(),
            Item::Text(_) | Item::Texts(_) => None,
        }
    }

    /// An array's items, in order; none for anything else.
    pub fn items(self) -> Items<'a> {
        let inner = match self {
            Item::Node(doc, at) => match doc.node(at) {
                Some(Node::Array { count, .. }) => Walk::Nodes {
                    doc,
                    at: at + 1,
                    left: count,
                },
                _ => Walk::Done,
            },
            Item::Json(Json::Array(items)) => Walk::Json(items.iter()),
            Item::Texts(texts) => Walk::Texts(texts.iter()),
            Item::Json(_) | Item::Text(_) => Walk::Done,
        };
        Items(inner)
    }

    /// An object's members; `None` for anything else.
    pub fn members(self) -> Option<Members<'a>> {
        let source = match self {
            Item::Node(doc, at) => match doc.node(at)? {
                Node::Object { .. } => Source::Node(doc, at),
                _ => return None,
            },
            Item::Json(Json::Object(members)) => Source::Json(members),
            Item::Json(_) | Item::Text(_) | Item::Texts(_) => return None,
        };
        Some(Members {
            source,
            headers: &[],
            untagged: false,
        })
    }

    /// The `serde_json` value [`super::parse`] gives for it.
    pub fn to_json(self) -> Json {
        match self {
            Item::Node(doc, at) => doc.json(at),
            Item::Json(json) => json.clone(),
            Item::Text(text) => Json::String(text.to_string()),
            Item::Texts(texts) => Json::Array(texts.iter().cloned().map(Json::String).collect()),
        }
    }

    /// The numbers of the document it is in, by where they stand from its
    /// top; none outside a document read in place.
    pub fn numbers(self) -> Option<Rc<Numbers>> {
        match self {
            Item::Node(doc, _) => Some(Rc::new(doc.numbers())),
            _ => None,
        }
    }
}

/// An array's items, as [`Item::items`] gives them.
#[derive(Debug)]
pub struct Items<'a>(Walk<'a>);

#[derive(Debug)]
enum Walk<'a> {
    Nodes {
        doc: &'a Doc<'a>,
        at: u32,
        left: u32,
    },
    Json(std::slice::Iter<'a, Json>),
    Texts(std::slice::Iter<'a, String>),
    Done,
}

impl<'a> Iterator for Items<'a> {
    type Item = Item<'a>;

    fn next(&mut self) -> Option<Item<'a>> {
        match &mut self.0 {
            Walk::Nodes { doc, at, left } => {
                if *left == 0 {
                    return None;
                }
                let item = Item::Node(doc, *at);
                *at = doc.after(*at);
                *left -= 1;
                Some(item)
            }
            Walk::Json(items) => items.next().map(Item::Json),
            Walk::Texts(texts) => texts.next().map(|text| Item::Text(text)),
            Walk::Done => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = match &self.0 {
            Walk::Nodes { left, .. } => *left as usize,
            Walk::Json(items) => items.len(),
            Walk::Texts(texts) => texts.len(),
            Walk::Done => 0,
        };
        (left, Some(left))
    }
}

impl ExactSizeIterator for Items<'_> {}

/// An object's members, wherever they come from, with the fields read
/// from headers in place of any member of their name (area 7, "One
/// input"), and without `tag` when they are a payload variant's.
#[derive(Clone, Copy, Debug)]
pub struct Members<'a> {
    source: Source<'a>,
    headers: &'a [(&'a str, Option<&'a str>)],
    untagged: bool,
}

#[derive(Clone, Copy, Debug)]
enum Source<'a> {
    Node(&'a Doc<'a>, u32),
    Json(&'a serde_json::Map<String, Json>),
    /// A query string's values, by key.
    Query(&'a BTreeMap<String, Vec<String>>),
    /// An object with no members: what a body that isn't an object reads
    /// as, once refused, so its header fields are still read.
    Empty,
}

impl<'a> Members<'a> {
    /// A query string's values, by key, as an object's members.
    pub fn query(values: &'a BTreeMap<String, Vec<String>>) -> Members<'a> {
        Members {
            source: Source::Query(values),
            headers: &[],
            untagged: false,
        }
    }

    /// An object with no members.
    pub fn empty() -> Members<'a> {
        Members {
            source: Source::Empty,
            headers: &[],
            untagged: false,
        }
    }

    /// The same members with `headers`' fields in place of any member of
    /// their name: each field's name and its header's value when sent.
    pub fn with_headers(self, headers: &'a [(&'a str, Option<&'a str>)]) -> Members<'a> {
        Members { headers, ..self }
    }

    /// The same members without `tag`: a payload variant's fields.
    pub fn untagged(self) -> Members<'a> {
        Members {
            untagged: true,
            ..self
        }
    }

    /// Whether the object itself has a member called `name`, whatever the
    /// headers give.
    pub fn contains(&self, name: &str) -> bool {
        match self.source {
            Source::Node(doc, at) => self.node_member(doc, at, name).is_some(),
            Source::Json(members) => members.contains_key(name),
            Source::Query(values) => values.contains_key(name),
            Source::Empty => false,
        }
    }

    /// Member `name`: a field read from a header given its header's value,
    /// and a query string's key its values, as an array for a `list`
    /// field or when given more than once, else as text.
    pub fn get(&self, name: &str, list: bool) -> Option<Item<'a>> {
        if self.untagged && name == "tag" {
            return None;
        }
        if let Some((_, value)) = self.headers.iter().find(|(field, _)| *field == name) {
            return value.map(Item::Text);
        }
        match self.source {
            Source::Node(doc, at) => self.node_member(doc, at, name),
            Source::Json(members) => members.get(name).map(Item::Json),
            Source::Query(values) => values.get(name).map(|given| match given.as_slice() {
                [one] if !list => Item::Text(one),
                all => Item::Texts(all),
            }),
            Source::Empty => None,
        }
    }

    fn node_member(&self, doc: &'a Doc<'a>, at: u32, name: &str) -> Option<Item<'a>> {
        let Some(Node::Object { count, .. }) = doc.node(at) else {
            return None;
        };
        let mut key = at + 1;
        for _ in 0..count {
            let value = key + 1;
            if let Some(Node::String {
                start,
                end,
                escaped,
            }) = doc.node(key)
                && doc.string(start, end, escaped) == name
            {
                return Some(Item::Node(doc, value));
            }
            key = doc.after(value);
        }
        None
    }

    /// The names of its members not in `declared`, in name order, as a
    /// `serde_json` object keeps them: what is refused as `unknown_field`.
    /// A field read from a header is declared, so never among them.
    pub fn unknown(&self, declared: &[&str]) -> Vec<Cow<'a, str>> {
        let fresh = |name: &str| !declared.contains(&name) && !(self.untagged && name == "tag");
        let mut names: Vec<Cow<'a, str>> = match self.source {
            Source::Node(doc, at) => {
                let mut names = Vec::new();
                if let Some(Node::Object { count, .. }) = doc.node(at) {
                    let mut key = at + 1;
                    for _ in 0..count {
                        if let Some(Node::String {
                            start,
                            end,
                            escaped,
                        }) = doc.node(key)
                        {
                            let name = doc.string(start, end, escaped);
                            if fresh(&name) {
                                names.push(name);
                            }
                        }
                        key = doc.after(key + 1);
                    }
                }
                names
            }
            Source::Json(members) => members
                .keys()
                .filter(|name| fresh(name))
                .map(|name| Cow::Borrowed(name.as_str()))
                .collect(),
            Source::Query(values) => values
                .keys()
                .filter(|name| fresh(name))
                .map(|name| Cow::Borrowed(name.as_str()))
                .collect(),
            Source::Empty => Vec::new(),
        };
        names.sort_unstable();
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A seeded stream of numbers (SplitMix64).
    struct Seeded(u64);

    impl Seeded {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    /// Pieces documents are made of: right and wrong, so most documents
    /// are near the edge of what reads.
    const PIECES: &[&str] = &[
        "{",
        "}",
        "[",
        "]",
        ",",
        ":",
        " ",
        "\n",
        "\t",
        "\"a\"",
        "\"b\"",
        "\"\\u0061\"",
        "\"\\\"\"",
        "\"\\ud800\"",
        "\"é\"",
        "\"\u{1}\"",
        "\"\\x\"",
        "\"\\",
        "0",
        "-0",
        "1",
        "-1",
        "1.5",
        "1e3",
        "1E-2",
        "01",
        "1.",
        ".5",
        "-",
        "1e400",
        "-1e400",
        "18446744073709551615",
        "18446744073709551616",
        "-9223372036854775808",
        "-9223372036854775809",
        "true",
        "false",
        "null",
        "tru",
        "nul",
        "x",
        "\"",
        "\\",
    ];

    fn made(seed: &mut Seeded) -> String {
        let mut text = String::new();
        for _ in 0..seed.below(14) {
            text.push_str(PIECES[seed.below(PIECES.len())]);
        }
        text
    }

    /// A well-formed document, nested up to `depth`, often with a member
    /// named twice.
    fn document(seed: &mut Seeded, depth: usize) -> String {
        match seed.below(if depth == 0 { 5 } else { 8 }) {
            0 => "null".into(),
            1 => ["0", "-0", "12", "-7", "1.25", "3e2", "9007199254740993"][seed.below(7)].into(),
            2 => ["\"\"", "\"x\"", "\"a\\nb\"", "\"\\u00e9\""][seed.below(4)].into(),
            3 => "true".into(),
            4 => "false".into(),
            5 | 6 => {
                let names = ["a", "b", "c", "\\u0061"];
                let members: Vec<String> = (0..seed.below(4))
                    .map(|_| {
                        format!(
                            "\"{}\": {}",
                            names[seed.below(names.len())],
                            document(seed, depth - 1)
                        )
                    })
                    .collect();
                format!("{{{}}}", members.join(", "))
            }
            _ => {
                let items: Vec<String> = (0..seed.below(4))
                    .map(|_| document(seed, depth - 1))
                    .collect();
                format!("[{}]", items.join(","))
            }
        }
    }

    /// The document read in place reads as `parse` reads it: the same
    /// refusals, the same values, the same numbers' texts. Whether it
    /// read.
    fn agrees(text: &str) -> bool {
        let parsed = super::super::parse(text.as_bytes());
        let read = Doc::parse(text.as_bytes());
        match (&parsed, &read) {
            (None, None) => false,
            (Some((json, numbers)), Some(doc)) => {
                assert_eq!(doc.root().to_json(), *json, "{text}");
                assert_eq!(doc.numbers(), *numbers, "{text}");
                true
            }
            _ => panic!(
                "{text:?}: parse {}, read in place {}",
                parsed.is_some(),
                read.is_some()
            ),
        }
    }

    #[test]
    fn a_document_read_in_place_reads_as_parse_reads_it() {
        let mut seed = Seeded(1);
        // Made of pieces, most don't read and some do; well formed, most
        // read and some name a member twice.
        let made_read = (0..40_000).filter(|_| agrees(&made(&mut seed))).count();
        let documents_read = (0..4_000)
            .filter(|_| agrees(&document(&mut seed, 4)))
            .count();
        assert!(made_read > 1_000, "{made_read}");
        assert!((2_000..4_000).contains(&documents_read), "{documents_read}");
        for text in [
            "",
            " ",
            "{}",
            "[]",
            "{\"a\":1,\"a\":2}",
            "{\"a\":1,\"\\u0061\":2}",
            "{\"a\":{\"a\":1},\"b\":{\"a\":2}}",
            "[1,]",
            "{\"a\":1,}",
            "\"\\ud83d\\ude00\"",
            "1 2",
            "  {\"a\" : [ 1 , 2 ] }  ",
        ] {
            agrees(text);
        }
        // As deep as `parse` reads, and no deeper.
        for depth in [127, 128, 129] {
            let read = depth <= 128;
            assert_eq!(
                agrees(&format!("{}{}", "[".repeat(depth), "]".repeat(depth))),
                read
            );
            assert_eq!(
                agrees(&format!(
                    "{}1{}",
                    "{\"a\":".repeat(depth),
                    "}".repeat(depth)
                )),
                read
            );
        }
    }

    #[test]
    fn many_members_are_checked_for_a_repeat_by_a_set() {
        let members: Vec<String> = (0..200).map(|n| format!("\"m{n}\":{n}")).collect();
        let fresh = format!("{{{}}}", members.join(","));
        assert!(agrees(&fresh));
        let repeated = format!("{{{},\"m17\":0}}", members.join(","));
        assert!(!agrees(&repeated));
        let escaped = format!("{{{},\"\\u006d17\":0}}", members.join(","));
        assert!(Doc::parse(escaped.as_bytes()).is_none());
    }

    #[test]
    fn a_whole_number_reads_as_serde_json_reads_it() {
        for written in [
            "0",
            "-0",
            "7",
            "-7",
            "1.0",
            "1e2",
            "9223372036854775807",
            "9223372036854775808",
            "-9223372036854775808",
            "-9223372036854775809",
            "18446744073709551615",
            "18446744073709551616",
        ] {
            let doc = Doc::parse(written.as_bytes()).unwrap();
            let json: Json = serde_json::from_str(written).unwrap();
            assert_eq!(doc.root().as_i64(), json.as_i64(), "{written}");
        }
    }

    #[test]
    fn members_are_found_by_name_and_listed_in_name_order() {
        let doc = Doc::parse(br#"{"b":1,"\u0061":"x","tag":"t","c":[1,2]}"#).unwrap();
        let members = doc.root().members().unwrap();
        assert_eq!(
            members.get("a", false).and_then(Item::text).as_deref(),
            Some("x")
        );
        assert_eq!(members.get("c", false).unwrap().items().len(), 2);
        assert_eq!(members.unknown(&["b"]), vec!["a", "c", "tag"]);
        assert_eq!(members.untagged().unknown(&["b"]), vec!["a", "c"]);
        assert!(members.untagged().get("tag", false).is_none());
        let headers = [("b", Some("from a header")), ("c", None)];
        let headed = members.with_headers(&headers);
        assert_eq!(
            headed.get("b", false).and_then(Item::text).as_deref(),
            Some("from a header")
        );
        assert!(headed.get("c", false).is_none());
        assert!(headed.contains("c"));
        let mut values = BTreeMap::new();
        values.insert("one".to_string(), vec!["1".to_string()]);
        values.insert("two".to_string(), vec!["1".to_string(), "2".to_string()]);
        let query = Members::query(&values);
        assert_eq!(query.get("one", false).unwrap().kind(), Kind::String);
        assert_eq!(query.get("one", true).unwrap().kind(), Kind::Array);
        assert_eq!(query.get("two", false).unwrap().kind(), Kind::Array);
    }
}
