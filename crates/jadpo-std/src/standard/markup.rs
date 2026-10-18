//! HTML made safe (area 19, "Text formats", chunk 5.5o): the sanitiser
//! every `Html` value is built with, the security boundary the packages
//! design (#88) asks for, and the text people read in it.
//!
//! An `Html` value is always safe to write into a page or an email
//! unescaped, because the only way to make one is through [`sanitise`]:
//! its construction from text, `text.parse`, a request's JSON and
//! `markup.to_html` all run it, and a literal written in code must already
//! be what it keeps ([`is_safe`]), which the checker proves while
//! compiling. It is an allow-list over the browsers' own parsing algorithm
//! (ammonia over html5ever): an element, an attribute or a link scheme
//! not listed here is dropped, never escaped into view, and the content of
//! the elements in [`CONTENT_DROPPED`] goes with them.
//!
//! The policy is the page's: no scripts, no event handlers, links only to
//! `https:` and `mailto:`. What it doesn't name is the most conservative
//! choice (area 19, "Chosen in chunk 5.5o"): basic formatting, headings,
//! lists, quotes, code and links; no images, tables, forms, frames, media,
//! styles or classes; no relative or protocol-relative links; and every
//! link `rel="noopener noreferrer nofollow"`.
//!
//! Sanitising is a fixed point: what [`sanitise`] gives, sanitised again,
//! is itself, so text parsed back from it (a browser reading the page, a
//! later construction) is the tree the allow-list kept. That is the defence
//! against mutation XSS, where markup that looks safe serialised becomes
//! something else when parsed again; output that isn't stable is cleaned
//! as escaped text instead, and empty when even that isn't.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use ammonia::{Builder, UrlRelative};
use html5ever::buffer_queue::BufferQueue;
use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{TagKind, Token, TokenSink, TokenSinkResult, Tokenizer};

/// The elements an `Html` value keeps, with the attributes each keeps.
/// Formatting, headings, lists, quotes, code, links and rules: what
/// CommonMark writes, less its images, and what a rich text editor's
/// basic marks are.
pub const ELEMENTS: &[(&str, &[&str])] = &[
    ("a", &["href", "title"]),
    ("abbr", &["title"]),
    ("b", &[]),
    ("blockquote", &[]),
    ("br", &[]),
    ("code", &[]),
    ("del", &[]),
    ("em", &[]),
    ("h1", &[]),
    ("h2", &[]),
    ("h3", &[]),
    ("h4", &[]),
    ("h5", &[]),
    ("h6", &[]),
    ("hr", &[]),
    ("i", &[]),
    ("ins", &[]),
    ("kbd", &[]),
    ("li", &[]),
    ("mark", &[]),
    ("ol", &["start"]),
    ("p", &[]),
    ("pre", &[]),
    ("q", &[]),
    ("s", &[]),
    ("small", &[]),
    ("strong", &[]),
    ("sub", &[]),
    ("sup", &[]),
    ("u", &[]),
    ("ul", &[]),
];

/// The schemes a link may have; any other, and a link with no scheme of
/// its own (relative, or protocol-relative `//host`), loses its `href`.
pub const LINK_SCHEMES: &[&str] = &["https", "mailto"];

/// What every link's `rel` is, whatever it was written with: the page it
/// opens gets no handle on this one (`noopener`), no `Referer`
/// (`noreferrer`), and no endorsement from it (`nofollow`).
pub const LINK_REL: &str = "noopener noreferrer nofollow";

/// The elements dropped with everything inside them, rather than leaving
/// their text behind: script and style, and every element whose content
/// is another language, raw text or a separate document (SVG, MathML,
/// templates, frames, plugins, form controls, `noscript`), where mutation
/// attacks hide markup.
pub const CONTENT_DROPPED: &[&str] = &[
    "applet", "embed", "frame", "frameset", "iframe", "math", "noembed", "noframes", "noscript",
    "object", "plaintext", "script", "select", "style", "svg", "template", "textarea", "title",
    "xmp",
];

/// The elements that start a new line in the text people read
/// ([`plain_text`]): every block the allow-list keeps.
const BLOCKS: &[&str] = &[
    "blockquote", "h1", "h2", "h3", "h4", "h5", "h6", "hr", "li", "ol", "p", "pre", "ul",
];

/// The sanitiser, built once from the policy above.
static SANITISER: LazyLock<Builder<'static>> = LazyLock::new(|| {
    let mut builder = Builder::empty();
    let tags: HashSet<&'static str> = ELEMENTS.iter().map(|(tag, _)| *tag).collect();
    let attributes: HashMap<&'static str, HashSet<&'static str>> = ELEMENTS
        .iter()
        .filter(|(_, attributes)| !attributes.is_empty())
        .map(|(tag, attributes)| (*tag, attributes.iter().copied().collect()))
        .collect();
    builder
        .tags(tags)
        .tag_attributes(attributes)
        .tag_attribute_values(HashMap::new())
        .set_tag_attribute_values(HashMap::new())
        .generic_attributes(HashSet::new())
        .generic_attribute_prefixes(HashSet::new())
        .allowed_classes(HashMap::new())
        .url_schemes(LINK_SCHEMES.iter().copied().collect())
        .url_relative(UrlRelative::Deny)
        .link_rel(Some(LINK_REL))
        .strip_comments(true)
        .id_prefix(None)
        .clean_content_tags(CONTENT_DROPPED.iter().copied().collect());
    builder
});

/// The deepest elements may nest in markup the sanitiser parses, as
/// Blink's parser caps it: the HTML parsing algorithm scans the open
/// elements at every tag, so nesting without a bound makes it quadratic, a
/// way to stall the program with one large value. Deeper markup is kept
/// as text.
pub const NESTING_LIMIT: usize = 512;

/// The elements that never hold others (HTML's void elements), and those
/// whose start closes one of their own kind still open (`<p>`, `<li>`),
/// so neither nests by repetition.
const NOT_NESTING: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "keygen", "link", "meta", "param",
    "source", "track", "wbr", "p", "li", "dt", "dd", "option", "optgroup", "tr", "td", "th",
    "thead", "tbody", "tfoot", "caption", "colgroup", "rb", "rt", "rp", "rtc",
];

/// How deep `html`'s elements can nest, counted from its tokens: one more
/// for each start tag of an element that nests, one fewer for each end
/// tag. An upper bound on the depth the parser reaches, read in one pass.
fn nesting(html: &str) -> usize {
    #[derive(Default)]
    struct Depth {
        now: std::cell::Cell<usize>,
        deepest: std::cell::Cell<usize>,
    }
    impl TokenSink for Depth {
        type Handle = ();
        fn process_token(&self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
            if let Token::TagToken(tag) = token {
                match tag.kind {
                    TagKind::StartTag if !NOT_NESTING.contains(&&*tag.name) => {
                        let now = self.now.get().saturating_add(1);
                        self.now.set(now);
                        self.deepest.set(self.deepest.get().max(now));
                    }
                    TagKind::StartTag => {}
                    TagKind::EndTag => self.now.set(self.now.get().saturating_sub(1)),
                }
            }
            TokenSinkResult::Continue
        }
    }
    let input = BufferQueue::default();
    input.push_back(StrTendril::from_slice(html));
    let tokenizer = Tokenizer::new(Depth::default(), Default::default());
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    tokenizer.sink.deepest.get()
}

/// One pass of the allow-list.
fn clean(html: &str) -> String {
    SANITISER.clean(html).to_string()
}

/// `html` made safe: only the allowed elements and attributes, links only
/// to `https:` and `mailto:`, no comments, the dropped elements' content
/// gone, and the text that is left escaped where HTML needs it; markup
/// nested deeper than [`NESTING_LIMIT`] is kept as text. The result is
/// always a fixed point, `sanitise(sanitise(x)) == sanitise(x)`.
pub fn sanitise(html: &str) -> String {
    if nesting(html) > NESTING_LIMIT {
        return as_text(html);
    }
    let once = clean(html);
    if clean(&once) == once {
        return once;
    }
    // Markup whose serialisation parses back as something else (mutation
    // XSS): keep its words, as text, and none of its markup.
    as_text(html)
}

/// `html` kept as the text it is written in, none of it markup; empty
/// when even that wouldn't read back as itself.
fn as_text(html: &str) -> String {
    let escaped = clean(&escape(html));
    if clean(&escaped) == escaped {
        return escaped;
    }
    String::new()
}

/// Whether `html` is already what the sanitiser keeps: an `Html` literal's
/// check, and the canonical text `text.parse` gives back unchanged.
pub fn is_safe(html: &str) -> bool {
    clean(html) == html
}

/// `text` with every character HTML gives a meaning escaped, so it reads
/// as the same text and never as markup.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// The text people read in `html` (`markup.to_plain_text`): its text
/// content after sanitising, each block on its own line, a line break
/// where it has `<br>`, white space run together as a browser shows it
/// (kept as written inside `<pre>`), each line without white space at
/// either end, and no empty lines.
///
/// It reads the sanitised text's tokens rather than a tree: what the
/// sanitiser keeps is balanced, and none of its elements changes how the
/// text after it is tokenised, so the tokens are the tree's in order.
pub fn plain_text(html: &str) -> String {
    let mut text = PlainText::default();
    for piece in pieces(&sanitise(html)) {
        match piece {
            Piece::Text(words) => text.push(&words),
            Piece::Start { name, .. } if name == "br" => text.line_break(),
            Piece::Start { name, .. } if BLOCKS.contains(&name.as_str()) => {
                text.end_block();
                if name == "pre" {
                    text.pre += 1;
                }
            }
            Piece::End(name) if BLOCKS.contains(&name.as_str()) => {
                text.end_block();
                if name == "pre" {
                    text.pre = text.pre.saturating_sub(1);
                }
            }
            Piece::Start { .. } | Piece::End(_) | Piece::Other => {}
        }
    }
    text.end_block();
    text.lines.join("\n")
}

/// One token of HTML as the browsers' tokeniser reads it: a start tag with
/// its attributes, an end tag, text with its character references
/// decoded, or anything else (a comment, a doctype).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    Start {
        name: String,
        attributes: Vec<(String, String)>,
    },
    End(String),
    Text(String),
    Other,
}

/// The tokens [`pieces`] collects.
#[derive(Default)]
struct Pieces(RefCell<Vec<Piece>>);

impl TokenSink for Pieces {
    type Handle = ();

    fn process_token(&self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        let piece = match token {
            Token::TagToken(tag) => match tag.kind {
                TagKind::StartTag => Piece::Start {
                    name: tag.name.to_string(),
                    attributes: tag
                        .attrs
                        .iter()
                        .map(|attr| {
                            let name = match &attr.name.prefix {
                                Some(prefix) => format!("{prefix}:{}", attr.name.local),
                                None => attr.name.local.to_string(),
                            };
                            (name, attr.value.to_string())
                        })
                        .collect(),
                },
                TagKind::EndTag => Piece::End(tag.name.to_string()),
            },
            Token::CharacterTokens(text) => Piece::Text(text.to_string()),
            Token::EOFToken | Token::ParseError(_) => return TokenSinkResult::Continue,
            Token::DoctypeToken(_) | Token::CommentToken(_) | Token::NullCharacterToken => {
                Piece::Other
            }
        };
        self.0.borrow_mut().push(piece);
        TokenSinkResult::Continue
    }
}

/// `html`'s tokens, in order.
fn pieces(html: &str) -> Vec<Piece> {
    let input = BufferQueue::default();
    input.push_back(StrTendril::from_slice(html));
    let tokenizer = Tokenizer::new(Pieces::default(), Default::default());
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    tokenizer.sink.0.take()
}

/// The lines [`plain_text`] has written, and the block it is in.
#[derive(Default)]
struct PlainText {
    lines: Vec<String>,
    block: String,
    /// How many `<pre>` elements the walk is inside.
    pre: usize,
}

impl PlainText {
    fn push(&mut self, text: &str) {
        if self.pre > 0 {
            self.block.push_str(text);
            return;
        }
        for c in text.chars() {
            if c.is_ascii_whitespace() {
                if !self.block.ends_with(' ') && !self.block.ends_with('\n') {
                    self.block.push(' ');
                }
            } else {
                self.block.push(c);
            }
        }
    }

    fn line_break(&mut self) {
        self.block.push('\n');
    }

    /// Ends the block the walk is in: inside `<pre>`, its lines as
    /// written, less any blank ones at either end; anywhere else, each
    /// line trimmed and the empty ones left out.
    fn end_block(&mut self) {
        let block = std::mem::take(&mut self.block);
        if self.pre > 0 {
            let lines: Vec<&str> = block.split('\n').collect();
            let blank = |line: &&str| line.trim().is_empty();
            let first = lines.iter().position(|line| !blank(line));
            let last = lines.iter().rposition(|line| !blank(line));
            if let (Some(first), Some(last)) = (first, last) {
                self.lines.extend(
                    lines
                        .get(first..=last)
                        .unwrap_or_default()
                        .iter()
                        .map(|line| line.trim_end().to_string()),
                );
            }
            return;
        }
        for line in block.split('\n') {
            let line = line.trim_matches(|c: char| c.is_ascii_whitespace());
            if !line.is_empty() {
                self.lines.push(line.to_string());
            }
        }
    }
}

/// Whether `ty` is the standard type whose construction is the sanitiser:
/// built from any text, never refusing it, keeping what [`sanitise`] keeps.
pub fn sanitises(ty: &str) -> bool {
    ty == HTML
}

/// `Html`'s name.
pub const HTML: &str = "Html";

/// `Markdown`'s name.
pub const MARKDOWN: &str = "Markdown";

#[cfg(test)]
mod tests;
