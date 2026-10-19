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
//! choice (area 19, "Text formats"): basic formatting, headings,
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
    "applet",
    "embed",
    "frame",
    "frameset",
    "iframe",
    "math",
    "noembed",
    "noframes",
    "noscript",
    "object",
    "plaintext",
    "script",
    "select",
    "style",
    "svg",
    "template",
    "textarea",
    "title",
    "xmp",
];

/// The elements that start a new line in the text people read
/// ([`plain_text`]): every block the allow-list keeps.
const BLOCKS: &[&str] = &[
    "blockquote",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "li",
    "ol",
    "p",
    "pre",
    "ul",
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
                text.close_block();
                if name == "pre" {
                    text.pre += 1;
                }
            }
            Piece::End(name) if BLOCKS.contains(&name.as_str()) => {
                text.close_block();
                if name == "pre" {
                    text.pre = text.pre.saturating_sub(1);
                }
            }
            Piece::Start { .. } | Piece::End(_) | Piece::Other => {}
        }
    }
    text.close_block();
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
    fn close_block(&mut self) {
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

// ---------------------------------------------------------------------
// Entries
// ---------------------------------------------------------------------

use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, StandardType, Storage, TextForm,
    Validator,
};
use crate::read::SqlType;

/// `Html`'s name.
pub const HTML: &str = "Html";

/// `Markdown`'s name.
pub const MARKDOWN: &str = "Markdown";

/// Any text: what the sanitiser is given, which it never refuses.
pub const ANY_TEXT: &str = r"[\s\S]*";

/// CommonMark's text (chunk 5.5o): anything but a control character other
/// than a tab, a line feed or a carriage return, so text from a form's
/// text area, whose lines end CRLF, is Markdown as it arrives.
pub const MARKDOWN_PATTERN: &str = r"[^\x00-\x08\x0B\x0C\x0E-\x1F\x7F]*";

/// A text format: held as text, sent and stored as it, never ordered, and
/// shown to people as the text they read in it.
const fn text_format(
    name: &'static str,
    pattern: &'static str,
    text: TextForm,
    validator: Option<Validator>,
    valid: &'static [(&'static str, &'static str)],
    malformed: &'static [&'static str],
) -> StandardType {
    StandardType {
        name,
        holds: Holds::Text {
            pattern,
            personal: None,
        },
        text: Some(text),
        json: Json::Text,
        storage: Storage::Column(SqlType::Text),
        format: Format {
            key: FormatKey::PlainText,
            options: &[],
        },
        ordered: false,
        validator,
        shape: None,
        examples: Examples {
            valid,
            format: None,
            malformed,
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
        signals: crate::standard::NO_SIGNALS,
    }
}

/// HTML that is always safe to write out (area 19, "Text formats"): built
/// from any text through [`sanitise`], so nothing is malformed.
pub const HTML_ENTRY: StandardType = text_format(
    HTML,
    ANY_TEXT,
    TextForm::Html,
    Some(Validator::Html),
    &[
        (
            "<p>Fresh <strong>soup</strong> daily.</p>",
            "Fresh soup daily.",
        ),
        ("Tom &amp; Jerry", "Tom & Jerry"),
        ("<ul><li>Leek</li><li>Tomato</li></ul>", "Leek\nTomato"),
        (
            "<a href=\"https://example.com/\" rel=\"noopener noreferrer nofollow\">our site</a>",
            "our site",
        ),
    ],
    &[],
);

/// CommonMark text (area 19, "Text formats"), made into `Html` by
/// `markup.to_html`.
pub const MARKDOWN_ENTRY: StandardType = text_format(
    MARKDOWN,
    MARKDOWN_PATTERN,
    TextForm::Checked,
    None,
    &[
        ("Fresh **soup** daily.", "Fresh soup daily."),
        ("# Soups\n\n- Leek\n- Tomato", "Soups\nLeek\nTomato"),
        ("Tom & Jerry", "Tom & Jerry"),
        (
            "Visit [our site](https://example.com/).\r\n",
            "Visit our site.",
        ),
    ],
    &["a\u{0}b", "ring the bell\u{7}"],
);

#[cfg(test)]
mod tests {
    //! The sanitiser's attack suite (area 19, "Text formats", chunk 5.5o):
    //! every vector comes out safe, judged not by ammonia but by parsing what
    //! it gives again, as a browser would, and holding every element,
    //! attribute and link in the tree to the policy, with the result a fixed
    //! point. The vectors are the OWASP XSS filter evasion cheat sheet's, the
    //! event handlers, `javascript:` hidden by entities and white space, SVG
    //! and MathML, nested and malformed tags, the published mutation-XSS
    //! bypasses of other sanitisers, `<base>`, `<meta refresh>`, CSS
    //! `expression` and `url()`, and protocol-relative and other links the
    //! policy refuses.

    use super::*;

    /// Whether `href` is a link the policy keeps: an absolute `https:` or
    /// `mailto:` URL, as the WHATWG URL parser a browser uses reads it.
    fn allowed_link(href: &str) -> bool {
        ammonia::Url::parse(href).is_ok_and(|url| LINK_SCHEMES.contains(&url.scheme()))
    }

    /// Why `output` isn't safe, if it isn't: an element, attribute, comment or
    /// link the policy doesn't keep, a `rel` that isn't the policy's, or text
    /// that changes when sanitised again.
    fn unsafe_because(output: &str) -> Option<String> {
        if sanitise(output) != output || !is_safe(output) {
            return Some(format!("`{output}` isn't a fixed point"));
        }
        // Read again as a browser's tokeniser reads it. None of the elements
        // the policy keeps changes how what follows is tokenised (no raw text,
        // no foreign content), so every tag a browser would build is here.
        for piece in pieces(output) {
            match piece {
                Piece::Start { name, attributes } => {
                    let Some((_, allowed)) = ELEMENTS.iter().find(|(kept, _)| *kept == name) else {
                        return Some(format!("the element `{name}`"));
                    };
                    for (key, value) in attributes {
                        if name == "a" && key == "rel" {
                            if value != LINK_REL {
                                return Some(format!("`rel=\"{value}\"`"));
                            }
                            continue;
                        }
                        if !allowed.contains(&key.as_str()) {
                            return Some(format!("the attribute `{key}` on `{name}`"));
                        }
                        if key == "href" && !allowed_link(&value) {
                            return Some(format!("the link `{value}`"));
                        }
                    }
                }
                Piece::End(name) => {
                    if !ELEMENTS.iter().any(|(kept, _)| *kept == name) {
                        return Some(format!("the end tag `{name}`"));
                    }
                }
                Piece::Other => return Some("a comment or a declaration".into()),
                Piece::Text(_) => {}
            }
        }
        // Nothing that runs, whatever context the text lands in.
        let lower = output.to_ascii_lowercase();
        for marker in ["<script", "<svg", "<math", "<style", "<iframe", "<img"] {
            if lower.contains(marker) {
                return Some(format!("`{marker}` in `{output}`"));
            }
        }
        None
    }

    fn assert_safe(vectors: &[&str]) {
        for vector in vectors {
            let output = sanitise(vector);
            if let Some(why) = unsafe_because(&output) {
                panic!("`{vector}` came out as `{output}`: {why}");
            }
        }
    }

    #[test]
    fn the_owasp_cheat_sheet_s_vectors_come_out_safe() {
        assert_safe(&[
            "<script>alert(1)</script>",
            "<SCRIPT SRC=https://xss.rocks/xss.js></SCRIPT>",
            "javascript:/*--></title></style></textarea></script></xmp><svg/onload='+/\"/+/onmouseover=1/+/[*/[]/+alert(1)//'>",
            "<IMG SRC=\"javascript:alert('XSS');\">",
            "<IMG SRC=javascript:alert('XSS')>",
            "<IMG SRC=JaVaScRiPt:alert('XSS')>",
            "<IMG SRC=`javascript:alert(\"RSnake says, 'XSS'\")`>",
            "<a onmouseover=\"alert(document.cookie)\">xxs link</a>",
            "<a onmouseover=alert(document.cookie)>xxs link</a>",
            "<IMG \"\"\"><SCRIPT>alert(\"XSS\")</SCRIPT>\"\\>",
            "<IMG SRC=javascript:alert(String.fromCharCode(88,83,83))>",
            "<IMG SRC=# onmouseover=\"alert('xxs')\">",
            "<IMG SRC= onmouseover=\"alert('xxs')\">",
            "<IMG onmouseover=\"alert('xxs')\">",
            "<IMG SRC=/ onerror=\"alert(String.fromCharCode(88,83,83))\"></img>",
            "<img src=x onerror=\"&#0000106&#0000097&#0000118&#0000097&#0000115&#0000099&#0000114&#0000105&#0000112&#0000116&#0000058&#0000097&#0000108&#0000101&#0000114&#0000116&#0000040&#0000039&#0000088&#0000083&#0000083&#0000039&#0000041\">",
            "<SCRIPT/XSS SRC=\"https://xss.rocks/xss.js\"></SCRIPT>",
            "<BODY onload!#$%&()*~+-_.,:;?@[/|\\]^`=alert(\"XSS\")>",
            "<SCRIPT/SRC=\"https://xss.rocks/xss.js\"></SCRIPT>",
            "<<SCRIPT>alert(\"XSS\");//\\<</SCRIPT>",
            "<SCRIPT SRC=https://xss.rocks/xss.js?< B >",
            "<SCRIPT SRC=//xss.rocks/.j>",
            "<IMG SRC=\"`<javascript:alert>`('XSS')\"",
            "<iframe src=https://xss.rocks/scriptlet.html <",
            "\\\";alert('XSS');//",
            "</script><script>alert('XSS');</script>",
            "</TITLE><SCRIPT>alert(\"XSS\");</SCRIPT>",
            "<INPUT TYPE=\"IMAGE\" SRC=\"javascript:alert('XSS');\">",
            "<BODY BACKGROUND=\"javascript:alert('XSS')\">",
            "<IMG DYNSRC=\"javascript:alert('XSS')\">",
            "<IMG LOWSRC=\"javascript:alert('XSS')\">",
            "<STYLE>li {list-style-image: url(\"javascript:alert('XSS')\");}</STYLE><UL><LI>XSS</br>",
            "<IMG SRC='vbscript:msgbox(\"XSS\")'>",
            "<IMG SRC=\"livescript:[code]\">",
            "<svg/onload=alert('XSS')>",
            "<BODY ONLOAD=alert('XSS')>",
            "<BGSOUND SRC=\"javascript:alert('XSS');\">",
            "<BR SIZE=\"&{alert('XSS')}\">",
            "<LINK REL=\"stylesheet\" HREF=\"javascript:alert('XSS');\">",
            "<LINK REL=\"stylesheet\" HREF=\"https://xss.rocks/xss.css\">",
            "<STYLE>@import'https://xss.rocks/xss.css';</STYLE>",
            "<META HTTP-EQUIV=\"Link\" Content=\"<https://xss.rocks/xss.css>; REL=stylesheet\">",
            "<STYLE>BODY{-moz-binding:url(\"https://xss.rocks/xssmoz.xml#xss\")}</STYLE>",
            "<STYLE>@im\\port'\\ja\\vasc\\ript:alert(\"XSS\")';</STYLE>",
            "<IMG STYLE=\"xss:expr/*XSS*/ession(alert('XSS'))\">",
            "<STYLE TYPE=\"text/javascript\">alert('XSS');</STYLE>",
            "<STYLE>.XSS{background-image:url(\"javascript:alert('XSS')\");}</STYLE><A CLASS=XSS></A>",
            "<STYLE type=\"text/css\">BODY{background:url(\"javascript:alert('XSS')\")}</STYLE>",
            "<XSS STYLE=\"xss:expression(alert('XSS'))\">",
            "<XSS STYLE=\"behavior: url(xss.htc);\">",
            "<IFRAME SRC=\"javascript:alert('XSS');\"></IFRAME>",
            "<IFRAME SRC=# onmouseover=\"alert(document.cookie)\"></IFRAME>",
            "<FRAMESET><FRAME SRC=\"javascript:alert('XSS');\"></FRAMESET>",
            "<TABLE BACKGROUND=\"javascript:alert('XSS')\">",
            "<TABLE><TD BACKGROUND=\"javascript:alert('XSS')\">",
            "<DIV STYLE=\"background-image: url(javascript:alert('XSS'))\">",
            "<DIV STYLE=\"background-image:\\0075\\0072\\006C\\0028'\\006a\\0061\\0076\\0061\\0073\\0063\\0072\\0069\\0070\\0074\\003a\\0061\\006c\\0065\\0072\\0074\\0028.1027\\0058.1053\\0053\\0027\\0029'\\0029\">",
            "<DIV STYLE=\"background-image: url(&#1;javascript:alert('XSS'))\">",
            "<DIV STYLE=\"width: expression(alert('XSS'));\">",
            "<!--[if gte IE 4]><SCRIPT>alert('XSS');</SCRIPT><![endif]-->",
            "<OBJECT TYPE=\"text/x-scriptlet\" DATA=\"https://xss.rocks/scriptlet.html\"></OBJECT>",
            "<EMBED SRC=\"data:image/svg+xml;base64,PHN2ZyB4bWxuczpzdmc9Imh0dH A6Ly93d3cudzMub3JnLzIwMDAvc3ZnIiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcv MjAwMC9zdmciIHhtbG5zOnhsaW5rPSJodHRwOi8vd3d3LnczLm9yZy8xOTk5L3hs aW5rIiB2ZXJzaW9uPSIxLjAiIHg9IjAiIHk9IjAiIHdpZHRoPSIxOTQiIGhlaWdodD0iMjAw IiBpZD0ieHNzIj48c2NyaXB0IHR5cGU9InRleHQvZWNtYXNjcmlwdCI+YWxlcnQoIlh TUyIpOzwvc2NyaXB0Pjwvc3ZnPg==\" type=\"image/svg+xml\" AllowScriptAccess=\"always\"></EMBED>",
            "<SCRIPT a=\">\" SRC=\"httx://xss.rocks/xss.js\"></SCRIPT>",
            "<SCRIPT =\">\" SRC=\"httx://xss.rocks/xss.js\"></SCRIPT>",
            "<SCRIPT \"a='>'\" SRC=\"httx://xss.rocks/xss.js\"></SCRIPT>",
            "<SCRIPT>document.write(\"<SCRI\");</SCRIPT>PT SRC=\"httx://xss.rocks/xss.js\"></SCRIPT>",
            "<A HREF=\"http://66.102.7.147/\">XSS</A>",
            "<A HREF=\"http://%77%77%77%2E%67%6F%6F%67%6C%65%2E%63%6F%6D\">XSS</A>",
            "<A HREF=\"h\ntt\tp://6\t6.000146.0x7.147/\">XSS</A>",
            "<A HREF=\"//www.google.com/\">XSS</A>",
            "<A HREF=\"javascript:document.location='http://www.google.com/'\">XSS</A>",
            "+ADw-SCRIPT+AD4-alert('XSS');+ADw-/SCRIPT+AD4-",
        ]);
    }

    #[test]
    fn event_handlers_are_dropped_wherever_they_are_written() {
        assert_safe(&[
            "<p onclick=\"alert(1)\" onmouseover=alert(1)>x</p>",
            "<b/onclick=alert(1)>x</b>",
            "<p\tonclick=alert(1)>x</p>",
            "<p\nonclick=alert(1)>x</p>",
            "<p/onclick=alert(1)/>x</p>",
            "<a href=\"https://example.com/\" onclick=\"steal()\">x</a>",
            "<input autofocus onfocus=alert(1)>",
            "<details open ontoggle=alert(1)>",
            "<video><source onerror=\"alert(1)\">",
            "<audio src=x onerror=alert(1)>",
            "<body onpageshow=alert(1)>",
            "<marquee onstart=alert(1)>x</marquee>",
            "<select autofocus onfocus=alert(1)>",
            "<textarea autofocus onfocus=alert(1)>",
            "<keygen autofocus onfocus=alert(1)>",
            "<isindex type=image src=1 onerror=alert(1)>",
            "<div onpointerover=alert(1)>x</div>",
            "<strong ONCLICK=alert(1)>x</strong>",
            "<em on\u{0}click=alert(1)>x</em>",
        ]);
        assert_eq!(
            sanitise("<p onclick=\"alert(1)\">Hello <b onmouseover=x>there</b></p>"),
            "<p>Hello <b>there</b></p>"
        );
    }

    #[test]
    fn javascript_links_hidden_by_entities_and_white_space_lose_their_href() {
        let hidden = [
            "<a href=\"javascript:alert(1)\">x</a>",
            "<a href=\"JaVaScRiPt:alert(1)\">x</a>",
            "<a href=\" javascript:alert(1)\">x</a>",
            "<a href=\"&#106;&#97;&#118;&#97;&#115;&#99;&#114;&#105;&#112;&#116;&#58;&#97;&#108;&#101;&#114;&#116;&#40;&#39;&#88;&#83;&#83;&#39;&#41;\">x</a>",
            "<a href=\"&#0000106&#0000097&#0000118&#0000097&#0000115&#0000099&#0000114&#0000105&#0000112&#0000116&#0000058&#0000097&#0000108&#0000101&#0000114&#0000116&#0000040&#0000039&#0000088&#0000083&#0000083&#0000039&#0000041\">x</a>",
            "<a href=\"&#x6A&#x61&#x76&#x61&#x73&#x63&#x72&#x69&#x70&#x74&#x3A&#x61&#x6C&#x65&#x72&#x74&#x28&#x27&#x58&#x53&#x53&#x27&#x29\">x</a>",
            "<a href=\"jav\tascript:alert('XSS');\">x</a>",
            "<a href=\"jav&#x09;ascript:alert('XSS');\">x</a>",
            "<a href=\"jav&#x0A;ascript:alert('XSS');\">x</a>",
            "<a href=\"jav&#x0D;ascript:alert('XSS');\">x</a>",
            "<a href=\" &#14;  javascript:alert('XSS');\">x</a>",
            "<a href=\"java\u{0}script:alert(1)\">x</a>",
            "<a href=\"javascript&colon;alert(1)\">x</a>",
            "<a href=\"java&Tab;script:alert(1)\">x</a>",
            "<a href=\"java&NewLine;script:alert(1)\">x</a>",
            "<a href=\"\u{FF4A}avascript:alert(1)\">x</a>",
            "<a href=\"javascript://%0Aalert(1)\">x</a>",
            "<a href=\"javascript:alert%281%29\">x</a>",
            "<a href='javascript:alert(1)'>x</a>",
            "<a href=javascript:alert(1)>x</a>",
            "<a href=\"vbscript:msgbox(1)\">x</a>",
            "<a href=\"data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==\">x</a>",
            "<a href=\"data:text/html,<script>alert(1)</script>\">x</a>",
            "<a href=\"DATA:text/html,<script>alert(1)</script>\">x</a>",
            "<a href=\"file:///etc/passwd\">x</a>",
            "<a href=\"blob:https://example.com/uuid\">x</a>",
        ];
        assert_safe(&hidden);
        for vector in hidden {
            assert_eq!(
                sanitise(vector),
                "<a rel=\"noopener noreferrer nofollow\">x</a>",
                "{vector}"
            );
        }
    }

    #[test]
    fn only_https_and_mailto_links_are_kept() {
        assert_eq!(
            sanitise(
                "<a href=\"https://example.com/a?b=1&amp;c=2\" target=\"_blank\" rel=\"opener\">x</a>"
            ),
            "<a href=\"https://example.com/a?b=1&amp;c=2\" rel=\"noopener noreferrer nofollow\">x</a>"
        );
        assert_eq!(
            sanitise("<a href=\"mailto:help@example.com?subject=Hi\">Mail us</a>"),
            "<a href=\"mailto:help@example.com?subject=Hi\" rel=\"noopener noreferrer nofollow\">Mail us</a>"
        );
        let refused = [
            // Protocol-relative: a host of the attacker's choosing.
            "<a href=\"//evil.example/\">x</a>",
            "<a href=\"\\\\evil.example/\">x</a>",
            "<a href=\"/\\evil.example\">x</a>",
            "<a href=\"\\/evil.example\">x</a>",
            // Relative, and plain `http:`, which the page leaves out.
            "<a href=\"/account/delete\">x</a>",
            "<a href=\"account\">x</a>",
            "<a href=\"#top\">x</a>",
            "<a href=\"?q=1\">x</a>",
            "<a href=\"http://example.com/\">x</a>",
            "<a href=\"ftp://example.com/\">x</a>",
            "<a href=\"tel:+441234567890\">x</a>",
        ];
        assert_safe(&refused);
        for vector in refused {
            let output = sanitise(vector);
            assert!(
                !output.contains("href"),
                "`{vector}` kept its link: `{output}`"
            );
        }
    }

    #[test]
    fn svg_and_mathml_are_dropped_with_their_content() {
        assert_safe(&[
            "<svg><script>alert(1)</script></svg>",
            "<svg onload=alert(1)>",
            "<svg><a xlink:href=\"javascript:alert(1)\"><text x=\"20\" y=\"20\">XSS</text></a></svg>",
            "<svg><animate attributeName=\"href\" values=\"javascript:alert(1)\"/></svg>",
            "<svg><set attributeName=\"href\" to=\"javascript:alert(1)\"/></svg>",
            "<svg><use href=\"data:image/svg+xml,<svg id='x' xmlns='http://www.w3.org/2000/svg'><image href='1' onerror='alert(1)' /></svg>#x\" /></svg>",
            "<svg><foreignObject><iframe srcdoc=\"<script>alert(1)</script>\"></iframe></foreignObject></svg>",
            "<svg><style>@import url(javascript:alert(1))</style></svg>",
            "<math href=\"javascript:alert(1)\">CLICKME</math>",
            "<math><maction actiontype=\"statusline#http://google.com\" xlink:href=\"javascript:alert(1)\">CLICKME</maction></math>",
            "<math><mtext><img src=x onerror=alert(1)></mtext></math>",
            "<math><annotation-xml encoding=\"text/html\"><img src=x onerror=alert(1)></annotation-xml></math>",
            "<svg><desc><img src=x onerror=alert(1)></desc></svg>",
        ]);
        assert_eq!(
            sanitise("before<svg><text>inside</text></svg>after"),
            "beforeafter"
        );
        assert_eq!(sanitise("a<math><mi>x</mi></math>b"), "ab");
    }

    #[test]
    fn nested_and_malformed_tags_come_out_well_formed() {
        assert_safe(&[
            "<<b>>x<</b>>",
            "<b><i>x</b></i>",
            "<p><div>x</p></div>",
            "<a href=\"https://a.example/\"><a href=\"javascript:x\">y</a></a>",
            "<scr<script>ipt>alert(1)</scr</script>ipt>",
            "<script<script>>alert(1)<</script>/script>",
            "<p title=\"",
            "<img src=\"x",
            "\"><script>alert(1)</script>",
            "'><script>alert(1)</script>",
            "</p><script>alert(1)</script>",
            "<scr\u{0}ipt>alert(1)</scr\u{0}ipt>",
            "<a href=\"https://example.com/\"<script>alert(1)</script>x</a>",
            "<a <script>alert(1)</script>x</a>",
            "<p><!-- <script>alert(1)</script> -->x</p>",
            "<!--><script>alert(1)</script>-->",
            "<!--!><script>alert(1)</script>-->",
            "<![CDATA[<script>alert(1)</script>]]>",
            "<?xml version=\"1.0\"?><script>alert(1)</script>",
            "<!DOCTYPE html><html><head><script>alert(1)</script></head><body>x</body></html>",
            "<template><script>alert(1)</script></template>",
            "<template><img src=x onerror=alert(1)></template>",
            "<style><img src=x onerror=alert(1)></style>",
            "<textarea><script>alert(1)</script></textarea>",
            "<title><script>alert(1)</script></title>",
            "<xmp><script>alert(1)</script></xmp>",
            "<plaintext><script>alert(1)</script>",
            "<iframe srcdoc=\"<script>alert(1)</script>\"></iframe>",
            "<object data=\"javascript:alert(1)\">",
            "<form action=\"javascript:alert(1)\"><button>x</button></form>",
            "<button formaction=\"javascript:alert(1)\">x</button>",
            "<form id=\"test\"></form><button form=\"test\" formaction=\"javascript:alert(1)\">X</button>",
            "<a id=\"location\" name=\"cookie\" class=\"x\" style=\"color:red\">x</a>",
            "<img src=\"https://example.com/cat.png\" alt=\"cat\">",
        ]);
        assert_eq!(sanitise("<b><i>x</b></i>"), "<b><i>x</i></b>");
        assert_eq!(sanitise("<p>unclosed"), "<p>unclosed</p>");
        assert_eq!(sanitise("<template>gone</template>kept"), "kept");
        assert_eq!(sanitise("<style>p { color: red }</style>kept"), "kept");
    }

    #[test]
    fn mutation_xss_bypasses_of_other_sanitisers_come_out_safe() {
        assert_safe(&[
            // DOMPurify 2.0.0's namespace confusion (Michał Bentkowski, 2019).
            "<svg></p><style><a id=\"</style><img src=1 onerror=alert(1)>\">",
            // DOMPurify 2.0.17's (2020).
            "<form><math><mtext></form><form><mglyph><style></math><img src onerror=alert(1)>",
            "<math><mtext><table><mglyph><style><!--</style><img title=\"--&gt;&lt;/mglyph&gt;&lt;img&Tab;src=1&Tab;onerror=alert(1)&gt;\">",
            "<math><mi><mglyph><svg><mtext><textarea><path id=\"</textarea><img onerror=alert(1) src=1>\">",
            // ammonia's own past advisories (RUSTSEC-2021-0074, RUSTSEC-2025):
            // content kept after removing a foreign element.
            "<svg><iframe><a title=\"</iframe><img src onerror=alert(1)>\">test",
            "<math><style><img src=x onerror=alert(1)></style></math>",
            // noscript parsed one way with scripting and another without.
            "<noscript><p title=\"</noscript><img src=x onerror=alert(1)>\">",
            "<noscript><style></noscript><img src=x onerror=alert(1)></style></noscript>",
            // Backticks and quotes in attribute values (IE's mXSS).
            "<p title=\"``onerror=alert(1)\">x</p>",
            "<a title=\"&lt;/a&gt;&lt;img src=x onerror=alert(1)&gt;\" href=\"https://example.com/\">x</a>",
            // Comments that end differently once serialised.
            "<!--a--!><img src=x onerror=alert(1)>-->",
            "<p><!--</p><img src=x onerror=alert(1)>--></p>",
            // Table foster parenting moving markup out.
            "<table><b><img src=x onerror=alert(1)></b></table>",
            "<table><a href=\"javascript:alert(1)\">x</a><tr><td>y</td></tr></table>",
            // `<select>` and `<option>`'s own parsing.
            "<select><style></select><img src=x onerror=alert(1)></style></select>",
            "<select><template><style></template><img src=x onerror=alert(1)>",
        ]);
    }

    #[test]
    fn base_meta_and_css_attacks_come_out_safe() {
        let vectors = [
            "<base href=\"javascript:alert('XSS');//\">",
            "<base href=\"https://evil.example/\"><a href=\"https://example.com/\">x</a>",
            "<meta http-equiv=\"refresh\" content=\"0;url=javascript:alert('XSS');\">",
            "<meta http-equiv=\"refresh\" content=\"0;url=data:text/html base64,PHNjcmlwdD5hbGVydCgnWFNTJyk8L3NjcmlwdD4K\">",
            "<meta http-equiv=\"refresh\" content=\"0; URL=https://;URL=javascript:alert('XSS');\">",
            "<meta charset=\"x-imap4-modified-utf7\">&ADz&AGn&AG0&AEf&ACA&AHM&AHI&AGO&AD0&AGn&ACA&AG8Abg&AGUAcgByAG8AcgA9AGEAbABlAHIAdAAoADEAKQ&ACAAPABi>",
            "<p style=\"background:url(javascript:alert(1))\">x</p>",
            "<p style=\"width: expression(alert('XSS'));\">x</p>",
            "<p style=\"background-image: url('https://evil.example/track.png')\">x</p>",
            "<p style=\"behavior:url(#default#time2)\">x</p>",
            "<p style=\"-moz-binding:url(https://evil.example/x.xml#x)\">x</p>",
            "<link rel=\"import\" href=\"https://evil.example/x.html\">",
            "<style>*{x:expression(alert(1))}</style>",
        ];
        assert_safe(&vectors);
        for vector in vectors {
            let output = sanitise(vector).to_ascii_lowercase();
            for kept in ["base", "meta", "style", "link", "expression", "url("] {
                assert!(
                    !output.contains(&format!("<{kept}")) && !output.contains(&format!("{kept}=")),
                    "`{vector}` kept `{kept}`: `{output}`"
                );
            }
        }
        assert_eq!(
            sanitise("<base href=\"https://evil.example/\"><a href=\"https://example.com/\">x</a>"),
            "<a href=\"https://example.com/\" rel=\"noopener noreferrer nofollow\">x</a>"
        );
    }

    #[test]
    fn deep_and_long_input_is_sanitised_without_failing() {
        // Nested within the limit: kept.
        let nested = format!(
            "{}x{}",
            "<blockquote>".repeat(500),
            "</blockquote>".repeat(500)
        );
        assert_safe(&[&nested]);
        assert!(sanitise(&nested).starts_with("<blockquote><blockquote>"));
        // Deeper: kept as text, read in one linear pass rather than parsed
        // in quadratic time (20,000 nested lists took over 9 seconds to
        // parse).
        for deep in [
            format!("{}x", "<blockquote>".repeat(2_000)),
            "<ul>".repeat(5_000),
            "<div><a href=\"javascript:x\">".repeat(1_000),
            format!("{}x{}", "<b>".repeat(2_000), "</b>".repeat(2_000)),
        ] {
            let output = sanitise(&deep);
            assert!(unsafe_because(&output).is_none(), "{output:.80}");
            assert!(!output.contains('<'), "kept as text: {output:.80}");
        }
        // Many siblings, and many attributes, are no deeper.
        let siblings = format!("{}{}", "<p>para".repeat(5_000), "<li>item".repeat(5_000));
        assert_safe(&[&siblings]);
        assert!(sanitise(&siblings).starts_with("<p>para</p><p>para</p>"));
        assert!(sanitise(&siblings).ends_with("<li>item</li><li>item</li>"));
        let attributes = format!("<p {}>x</p>", "onclick=alert(1) ".repeat(10_000));
        assert_eq!(sanitise(&attributes), "<p>x</p>");
        assert_eq!(plain_text(&nested), "x");
    }

    #[test]
    fn safe_html_is_kept_as_written() {
        let kept = [
            "<p>Hello, <strong>world</strong>.</p>",
            "<h2>Opening hours</h2><ul><li>Mon–Fri</li><li>Sat <em>(mornings)</em></li></ul>",
            "<ol start=\"3\"><li>three</li></ol>",
            "<blockquote><p>Quoted</p></blockquote><pre><code>let x = 1;\n</code></pre>",
            "Tom &amp; Jerry &lt;3",
            "line one<br>line two<hr>",
            "<p><abbr title=\"HyperText Markup Language\">HTML</abbr> <del>old</del> <ins>new</ins></p>",
            "<a href=\"https://example.com/\" rel=\"noopener noreferrer nofollow\">a link</a>",
            "",
        ];
        for html in kept {
            assert!(is_safe(html), "`{html}` is safe as written");
            assert_eq!(sanitise(html), html);
        }
        // Text that HTML would read differently is escaped, so it isn't safe
        // as written: the sanitiser writes it the one way.
        assert!(!is_safe("Tom & Jerry"));
        assert_eq!(sanitise("Tom & Jerry"), "Tom &amp; Jerry");
        assert!(!is_safe("<p>x"));
        assert!(
            !is_safe("<a href=\"https://example.com/\">x</a>"),
            "a link has its `rel`"
        );
    }

    #[test]
    fn plain_text_is_each_block_on_its_own_line() {
        assert_eq!(
            plain_text(
                "<h1>Menu</h1><p>Soup   of the\nday</p><ul><li>Tomato</li><li>Leek <b>and</b> potato</li></ul>"
            ),
            "Menu\nSoup of the day\nTomato\nLeek and potato"
        );
        assert_eq!(plain_text("one<br>two"), "one\ntwo");
        assert_eq!(plain_text("Tom &amp; Jerry"), "Tom & Jerry");
        assert_eq!(
            plain_text("<pre><code>fn main() {\n    run();\n}</code></pre>"),
            "fn main() {\n    run();\n}"
        );
        assert_eq!(plain_text("<script>alert(1)</script>Hello"), "Hello");
        assert_eq!(plain_text(""), "");
    }
}
