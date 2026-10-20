//! A JSON document's list of items, as `json.read`'s file form reads it
//! (area 19, "Data formats"; area 12, "How a file's run is kept"; chunk
//! 6.4b3): pure, fed the file's text a window at a time, keeping only
//! where it is in the document and the item in hand, so a document of any
//! size is walked in the memory of its largest item.
//!
//! - **Where the list is.** `items_at:` names the members leading from
//!   the document's top object to the list (`data.items`); with none, the
//!   document is the list. Each object on the way is read for its members'
//!   names, one named twice being refused as the in-memory reader refuses
//!   it; every other value is passed over, its brackets matched, its
//!   strings' quotes and escapes followed and its numbers and words held
//!   to JSON's.
//! - **Each item** is the text of one value in the list, from its first
//!   character to its last, which the row type's codec reads as JSON Lines'
//!   lines are read: a value that isn't JSON, or isn't the row type, is
//!   that item's problem, and the rest carry on.
//! - **What stops the run**, at the place it is: no list where `items_at:`
//!   says, a document whose structure isn't JSON (named by its line), one
//!   that ends before its JSON does, an object on the way naming a member
//!   twice, text that isn't text, and an item longer than the most a row
//!   may be. After the list the rest of the document is walked to its end,
//!   so a document cut short or followed by more is refused too.
//!
//! A walker that reached the list is kept, and a page starts from a copy
//! of it set to its own position ([`Walker::at_item`]), so a run resumes
//! at the byte its last page committed.

use super::{Encoding, Next, RowFault, next};

/// One item of the list: its text, the line it begins on, and where the
/// next begins: the byte after it and that byte's line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub text: String,
    pub line: u64,
    pub next: u64,
    pub next_line: u64,
}

/// Why a walker stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Walked {
    /// The list begins: the walker stands just after its `[`.
    Opened,
    /// It has as many items as it was asked for.
    Full,
    /// The window ended inside the document: feed it more.
    More,
    /// The document ended, whole.
    End,
    /// The file can't be read on from here.
    Fault(RowFault),
}

/// What is open around the walker.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Frame {
    /// An object. `level` is its place on the way to the list, when it is
    /// on the way: its members' names so far, whether the member being
    /// read is the one on the way, and whether that member was read.
    Object {
        level: Option<usize>,
        names: Vec<String>,
        on_way: bool,
        found: bool,
    },
    /// A list: the list of items, or one passed over.
    List { items: bool },
}

/// What may come next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expect {
    Value,
    ValueOrClose,
    Key,
    KeyOrClose,
    Colon,
    CommaOrClose,
    /// After the document's value: white space only.
    Nothing,
}

/// The token in hand.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Lex {
    Between,
    /// A string: whether a backslash came last, and, for a member's name
    /// on the way to the list, its text so far as written.
    Text {
        escaped: bool,
        key: bool,
        name: Option<String>,
    },
    /// A number, `true`, `false` or `null`, as written so far.
    Word(String),
}

/// The item being read.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Capture {
    text: String,
    line: u64,
    /// How many frames are open around the list's items.
    depth: usize,
}

/// A walk through one JSON document to its list of items and on to its
/// end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Walker {
    path: Vec<String>,
    encoding: Encoding,
    /// The most bytes an item may hold.
    longest: usize,
    frames: Vec<Frame>,
    expect: Expect,
    lex: Lex,
    /// The byte the walker reads next, and its line.
    at: u64,
    line: u64,
    item: Option<Capture>,
}

impl Walker {
    /// A walk from the document's first byte, `at`, after any byte-order
    /// mark, to the list `path` names, each item at most `longest` bytes.
    pub fn new(path: &[String], encoding: Encoding, at: u64, longest: usize) -> Walker {
        Walker {
            path: path.to_vec(),
            encoding,
            longest,
            frames: Vec::new(),
            expect: Expect::Value,
            lex: Lex::Between,
            at,
            line: 1,
            item: None,
        }
    }

    /// Where the walker reads next, and its line.
    pub fn place(&self) -> (u64, u64) {
        (self.at, self.line)
    }

    /// A copy of a walker that has just opened the list, standing at byte
    /// `at` on `line`: at the list's first item, or just after an item.
    pub fn at_item(&self, at: u64, line: u64, after_an_item: bool) -> Walker {
        let mut walker = self.clone();
        walker.at = at;
        walker.line = line;
        walker.lex = Lex::Between;
        walker.item = None;
        walker.expect = if after_an_item {
            Expect::CommaOrClose
        } else {
            Expect::ValueOrClose
        };
        walker
    }

    /// Walks `bytes`, the file's text from the walker's place on, adding
    /// each item it finishes to `items` until it has `most`; `ends` says
    /// the file ends where `bytes` do. With why it stopped, how many of
    /// `bytes` it used.
    pub fn walk(
        &mut self,
        bytes: &[u8],
        ends: bool,
        most: usize,
        items: &mut Vec<Item>,
    ) -> (usize, Walked) {
        let mut used = 0usize;
        loop {
            let (c, width) = match next(bytes, used, self.encoding) {
                Next::Char(c, width) => (c, width),
                Next::Invalid => return (used, self.not_text()),
                Next::Partial if ends => return (used, self.not_text()),
                Next::Partial => return (used, Walked::More),
                Next::Ended if !ends => return (used, Walked::More),
                Next::Ended => {
                    // A word the file's end closes.
                    if matches!(self.lex, Lex::Word(_)) {
                        match self.word_done(most, items) {
                            Ok(true) => return (used, Walked::Full),
                            Ok(false) => {}
                            Err(fault) => return (used, Walked::Fault(fault)),
                        }
                    }
                    return if self.expect == Expect::Nothing && self.lex == Lex::Between {
                        (used, Walked::End)
                    } else {
                        (used, Walked::Fault(RowFault::Ended))
                    };
                }
            };
            // A word ends at the first character that can't be in it,
            // which is then read for itself.
            if matches!(self.lex, Lex::Word(_)) && !word_char(c) {
                match self.word_done(most, items) {
                    Ok(true) => return (used, Walked::Full),
                    Ok(false) => {}
                    Err(fault) => return (used, Walked::Fault(fault)),
                }
            }
            used += width;
            self.at += width as u64;
            if let Some(capture) = &mut self.item {
                capture.text.push(c);
                if capture.text.len() > self.longest {
                    let line = capture.line;
                    return (used, Walked::Fault(RowFault::TooLong { line }));
                }
            }
            let line = self.line;
            if c == '\n' {
                self.line += 1;
            }
            match self.step(c, line, most, items) {
                Ok(None) => {}
                Ok(Some(walked)) => return (used, walked),
                Err(fault) => return (used, Walked::Fault(fault)),
            }
        }
    }

    fn not_text(&self) -> Walked {
        Walked::Fault(RowFault::NotText {
            line: self.line,
            encoding: self.encoding,
        })
    }

    fn not_json(line: u64) -> RowFault {
        RowFault::NotJson { line }
    }

    /// One character `c`, on `line`, already counted.
    fn step(
        &mut self,
        c: char,
        line: u64,
        most: usize,
        items: &mut Vec<Item>,
    ) -> Result<Option<Walked>, RowFault> {
        match &mut self.lex {
            Lex::Text { escaped, key, name } => {
                if *escaped {
                    *escaped = false;
                    if let Some(name) = name {
                        name.push(c);
                    }
                    return Ok(None);
                }
                match c {
                    '\\' => {
                        *escaped = true;
                        if let Some(name) = name {
                            name.push(c);
                        }
                        Ok(None)
                    }
                    '"' => {
                        let (key, name) = (*key, name.take());
                        self.lex = Lex::Between;
                        if key {
                            self.key_done(name, line)?;
                            Ok(None)
                        } else {
                            self.value_done(most, items)
                                .map(|full| full.then_some(Walked::Full))
                        }
                    }
                    c => {
                        if let Some(name) = name {
                            name.push(c);
                        }
                        Ok(None)
                    }
                }
            }
            Lex::Word(word) => {
                word.push(c);
                if word.len() > self.longest {
                    return Err(Self::not_json(line));
                }
                Ok(None)
            }
            Lex::Between => self.between(c, line, most, items),
        }
    }

    /// A character between tokens.
    fn between(
        &mut self,
        c: char,
        line: u64,
        most: usize,
        items: &mut Vec<Item>,
    ) -> Result<Option<Walked>, RowFault> {
        if matches!(c, ' ' | '\t' | '\n' | '\r') {
            return Ok(None);
        }
        match (c, self.expect) {
            ('{' | '[', Expect::Value | Expect::ValueOrClose) => self.open(c == '[', line),
            ('"', Expect::Value | Expect::ValueOrClose) => {
                self.begin_value('"', line)?;
                self.lex = Lex::Text {
                    escaped: false,
                    key: false,
                    name: None,
                };
                Ok(None)
            }
            ('"', Expect::Key | Expect::KeyOrClose) => {
                let on_way = matches!(
                    self.frames.last(),
                    Some(Frame::Object { level: Some(_), .. })
                );
                self.lex = Lex::Text {
                    escaped: false,
                    key: true,
                    name: on_way.then(String::new),
                };
                Ok(None)
            }
            (':', Expect::Colon) => {
                self.expect = Expect::Value;
                Ok(None)
            }
            (',', Expect::CommaOrClose) => {
                self.expect = match self.frames.last() {
                    Some(Frame::Object { .. }) => Expect::Key,
                    _ => Expect::Value,
                };
                Ok(None)
            }
            ('}', Expect::KeyOrClose | Expect::CommaOrClose)
                if matches!(self.frames.last(), Some(Frame::Object { .. })) =>
            {
                if let Some(Frame::Object {
                    level: Some(_),
                    found: false,
                    ..
                }) = self.frames.pop()
                {
                    return Err(self.no_list());
                }
                self.value_done(most, items)
                    .map(|full| full.then_some(Walked::Full))
            }
            (']', Expect::ValueOrClose | Expect::CommaOrClose)
                if matches!(self.frames.last(), Some(Frame::List { .. })) =>
            {
                self.frames.pop();
                self.value_done(most, items)
                    .map(|full| full.then_some(Walked::Full))
            }
            (c, Expect::Value | Expect::ValueOrClose) if c == '-' || c.is_ascii_alphanumeric() => {
                self.begin_value(c, line)?;
                self.lex = Lex::Word(c.to_string());
                Ok(None)
            }
            _ => Err(Self::not_json(line)),
        }
    }

    /// The list not where `items_at:` says.
    fn no_list(&self) -> RowFault {
        if self.path.is_empty() {
            RowFault::NotList
        } else {
            RowFault::NoList(self.path.join("."))
        }
    }

    /// A value begins with `c`: where it is decides what it must be. The
    /// document's value is the list, or the object on the way to it; the
    /// member on the way's is the next object on the way, or the list; a
    /// value in the list is an item.
    fn begin_value(&mut self, c: char, line: u64) -> Result<(), RowFault> {
        let wanted = match self.frames.last() {
            None => Some(if self.path.is_empty() { '[' } else { '{' }),
            Some(Frame::Object {
                level: Some(level),
                on_way: true,
                ..
            }) => Some(if level + 1 < self.path.len() {
                '{'
            } else {
                '['
            }),
            Some(Frame::List { items: true }) => {
                if self.item.is_none() {
                    self.item = Some(Capture {
                        text: c.to_string(),
                        line,
                        depth: self.frames.len(),
                    });
                }
                None
            }
            _ => None,
        };
        match wanted {
            Some(wanted) if wanted != c => Err(self.no_list()),
            _ => Ok(()),
        }
    }

    /// `{` or `[` opens a value.
    fn open(&mut self, list: bool, line: u64) -> Result<Option<Walked>, RowFault> {
        let c = if list { '[' } else { '{' };
        // What decides whether it is on the way to the list.
        let level = match self.frames.last() {
            None if !self.path.is_empty() => Some(0),
            Some(Frame::Object {
                level: Some(level),
                on_way: true,
                ..
            }) => Some(level + 1),
            _ => None,
        };
        self.begin_value(c, line)?;
        if list {
            let items = match level {
                None => self.frames.is_empty() && self.path.is_empty(),
                Some(level) => level == self.path.len(),
            };
            self.frames.push(Frame::List { items });
            self.expect = Expect::ValueOrClose;
            if items {
                return Ok(Some(Walked::Opened));
            }
        } else {
            self.frames.push(Frame::Object {
                level: level.filter(|level| *level < self.path.len()),
                names: Vec::new(),
                on_way: false,
                found: false,
            });
            self.expect = Expect::KeyOrClose;
        }
        Ok(None)
    }

    /// A member's name was read: `name` as written, when its object is on
    /// the way to the list.
    fn key_done(&mut self, name: Option<String>, line: u64) -> Result<(), RowFault> {
        self.expect = Expect::Colon;
        let Some(written) = name else {
            return Ok(());
        };
        let Ok(name) = serde_json::from_str::<String>(&format!("\"{written}\"")) else {
            return Err(Self::not_json(line));
        };
        let path = self.path.clone();
        if let Some(Frame::Object {
            level: Some(level),
            names,
            on_way,
            ..
        }) = self.frames.last_mut()
        {
            if names.contains(&name) {
                return Err(RowFault::Twice(name));
            }
            *on_way = path.get(*level) == Some(&name);
            names.push(name);
        }
        Ok(())
    }

    /// A number or word ended: held to JSON's unless it is in an item,
    /// whose own codec reads it. Whether it filled the items asked for.
    fn word_done(&mut self, most: usize, items: &mut Vec<Item>) -> Result<bool, RowFault> {
        let Lex::Word(word) = std::mem::replace(&mut self.lex, Lex::Between) else {
            return Ok(false);
        };
        if self.item.is_none() && !is_json_word(&word) {
            return Err(Self::not_json(self.line));
        }
        self.value_done(most, items)
    }

    /// A value is done: an item, when it was one, and what may follow.
    /// Whether it filled the items asked for.
    fn value_done(&mut self, most: usize, items: &mut Vec<Item>) -> Result<bool, RowFault> {
        let mut full = false;
        if self
            .item
            .as_ref()
            .is_some_and(|capture| capture.depth == self.frames.len())
            && let Some(capture) = self.item.take()
        {
            items.push(Item {
                text: capture.text,
                line: capture.line,
                next: self.at,
                next_line: self.line,
            });
            full = items.len() >= most;
        }
        match self.frames.last_mut() {
            None => self.expect = Expect::Nothing,
            Some(Frame::Object { on_way, found, .. }) => {
                if *on_way {
                    *found = true;
                    *on_way = false;
                }
                self.expect = Expect::CommaOrClose;
            }
            Some(Frame::List { .. }) => self.expect = Expect::CommaOrClose,
        }
        Ok(full)
    }
}

/// Whether `c` may be in a number or a word.
fn word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '+' | '.')
}

/// Whether `word` is `true`, `false`, `null` or a JSON number.
fn is_json_word(word: &str) -> bool {
    if matches!(word, "true" | "false" | "null") {
        return true;
    }
    let bytes = word.as_bytes();
    let mut at = 0usize;
    let digit = |at: usize| bytes.get(at).is_some_and(u8::is_ascii_digit);
    if bytes.first() == Some(&b'-') {
        at += 1;
    }
    match bytes.get(at) {
        Some(b'0') => at += 1,
        Some(b'1'..=b'9') => {
            while digit(at) {
                at += 1;
            }
        }
        _ => return false,
    }
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        if !digit(at) {
            return false;
        }
        while digit(at) {
            at += 1;
        }
    }
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        at += 1;
        if matches!(bytes.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        if !digit(at) {
            return false;
        }
        while digit(at) {
            at += 1;
        }
    }
    at == bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(written: &str) -> Vec<String> {
        if written.is_empty() {
            Vec::new()
        } else {
            written.split('.').map(str::to_string).collect()
        }
    }

    /// Walks `text` to its list at `at`, fed `window` bytes at a time,
    /// then on to its end, `most` items a page, each page from a copy of
    /// the walker at the list placed where the last page ended, as a run
    /// does: each item's text and line, and how it ended.
    fn walk_all(text: &str, at: &str, window: usize, most: usize) -> (Vec<(String, u64)>, Walked) {
        let bytes = text.as_bytes();
        let mut walker = Walker::new(&path(at), Encoding::Utf8, 0, 1000);
        let mut items = Vec::new();
        let mut fed = 0usize;
        // To the list.
        let opened = loop {
            let (start, _) = walker.place();
            let end = (fed + window).min(bytes.len());
            fed = end;
            let chunk = &bytes[start as usize..end];
            match walker.walk(chunk, end == bytes.len(), most, &mut items).1 {
                Walked::More => continue,
                Walked::Opened => break walker.clone(),
                other => return (Vec::new(), other),
            }
        };
        assert!(items.is_empty());
        let mut found: Vec<(String, u64)> = Vec::new();
        let (mut at, mut line) = opened.place();
        let mut after = false;
        loop {
            let mut page = opened.at_item(at, line, after);
            let mut items = Vec::new();
            let mut end = at as usize;
            let walked = loop {
                let (start, _) = page.place();
                end = (end + window).min(bytes.len());
                let chunk = &bytes[start as usize..end];
                match page.walk(chunk, end == bytes.len(), most, &mut items).1 {
                    Walked::More => continue,
                    other => break other,
                }
            };
            found.extend(items.iter().map(|item| (item.text.clone(), item.line)));
            if let Some(last) = items.last() {
                at = last.next;
                line = last.next_line;
                after = true;
            }
            if walked != Walked::Full {
                return (found, walked);
            }
        }
    }

    fn texts(found: &[(String, u64)]) -> Vec<&str> {
        found.iter().map(|(text, _)| text.as_str()).collect()
    }

    #[test]
    fn the_list_items_at_names_is_walked_however_the_text_is_cut() {
        let text = "{\n \"meta\": {\"note\": \"a [b] \\\"c\\\" {d}\", \"n\": [1, -2.5e3, {\"x\": null}]},\n \"data\": {\"items\": [\n  {\"code\": \"A1\", \"tags\": [\"]\"]},\n  7, \"x\", true, [1, 2],\n  {\"code\":\n \"A2\"}\n ], \"after\": false}\n}\n";
        for window in [1, 2, 5, 64, 1000] {
            for most in [1, 2, 10] {
                let (found, walked) = walk_all(text, "data.items", window, most);
                assert_eq!(walked, Walked::End, "{window} {most}");
                assert_eq!(
                    texts(&found),
                    [
                        "{\"code\": \"A1\", \"tags\": [\"]\"]}",
                        "7",
                        "\"x\"",
                        "true",
                        "[1, 2]",
                        "{\"code\":\n \"A2\"}",
                    ],
                    "{window} {most}"
                );
                assert_eq!(
                    found.iter().map(|(_, line)| *line).collect::<Vec<_>>(),
                    [4, 5, 5, 5, 5, 6]
                );
            }
        }
        // The document itself the list, and an empty one.
        let (found, walked) = walk_all(" [1 ,{\"a\": [] } ]\n", "", 3, 1);
        assert_eq!(
            (texts(&found), walked),
            (vec!["1", "{\"a\": [] }"], Walked::End)
        );
        let (found, walked) = walk_all("[]", "", 1, 1);
        assert_eq!((found.len(), walked), (0, Walked::End));
        // An item's own JSON is its codec's to judge.
        let (found, walked) = walk_all("[tru, {\"a\": 01}]", "", 4, 5);
        assert_eq!(
            (texts(&found), walked),
            (vec!["tru", "{\"a\": 01}"], Walked::End)
        );
    }

    #[test]
    fn what_isnt_where_items_at_says_or_isnt_json_stops_the_walk() {
        let no_list = || Walked::Fault(RowFault::NoList("data.items".into()));
        for (text, at, expected) in [
            ("{\"data\": {\"other\": []}}", "data.items", no_list()),
            ("{\"data\": {\"items\": {}}}", "data.items", no_list()),
            ("{\"data\": [1]}", "data.items", no_list()),
            ("[1]", "data.items", no_list()),
            ("{\"items\": []}", "", Walked::Fault(RowFault::NotList)),
            (
                "{\"a\": 1, \"a\": []}",
                "items",
                Walked::Fault(RowFault::Twice("a".into())),
            ),
            (
                "{\"a\": 1 \"items\": []}",
                "items",
                Walked::Fault(RowFault::NotJson { line: 1 }),
            ),
            (
                "{\"a\": tru, \"items\": []}",
                "items",
                Walked::Fault(RowFault::NotJson { line: 1 }),
            ),
            (
                "{\"a\": [1}, \"items\": []}",
                "items",
                Walked::Fault(RowFault::NotJson { line: 1 }),
            ),
            ("{\"items\":", "items", Walked::Fault(RowFault::Ended)),
            ("{\"it\\u0065ms\": [", "items", Walked::Opened),
        ] {
            let mut walker = Walker::new(&path(at), Encoding::Utf8, 0, 1000);
            let walked = walker.walk(text.as_bytes(), true, 10, &mut Vec::new()).1;
            assert_eq!(walked, expected, "{text}");
        }
        for (text, expected) in [
            ("[1,\n2\n3]", RowFault::NotJson { line: 3 }),
            ("[1,]", RowFault::NotJson { line: 1 }),
            ("[1, 2", RowFault::Ended),
            ("[1] x", RowFault::NotJson { line: 1 }),
            ("[1] [", RowFault::NotJson { line: 1 }),
            (
                "[\"a\", \"\u{0}\"]",
                RowFault::NotText {
                    line: 1,
                    encoding: Encoding::Utf8,
                },
            ),
        ] {
            assert_eq!(
                walk_all(text, "", 2, 10).1,
                Walked::Fault(expected),
                "{text}"
            );
        }
        // Items before a fault are walked.
        let (found, _) = walk_all("[1,\n2\n3]", "", 2, 10);
        assert_eq!(texts(&found), ["1", "2"]);
        // A member on the way named twice after the list.
        let (found, walked) = walk_all("{\"items\": [5], \"items\": []}", "items", 3, 10);
        assert_eq!(
            (texts(&found), walked),
            (vec!["5"], Walked::Fault(RowFault::Twice("items".into())))
        );
        // An item longer than the most a row may be.
        let mut walker = Walker::new(&[], Encoding::Utf8, 0, 8);
        let mut items = Vec::new();
        assert_eq!(walker.walk(b"[", true, 10, &mut items).1, Walked::Opened);
        let mut page = walker.at_item(1, 1, false);
        assert_eq!(
            page.walk(b"\"a long item\"]", true, 10, &mut items).1,
            Walked::Fault(RowFault::TooLong { line: 1 })
        );
    }

    #[test]
    fn a_number_is_held_to_jsons_grammar() {
        for word in [
            "0", "-0", "12", "1.5", "-2.5e-3", "1E10", "true", "false", "null",
        ] {
            assert!(is_json_word(word), "{word}");
        }
        for word in ["01", "1.", ".5", "+1", "1e", "--1", "tru", "nul", "0x1", ""] {
            assert!(!is_json_word(word), "{word}");
        }
    }
}
