//! Rich and localised text (area 19, "Text formats", chunk 5.5o2):
//! `RichText`, a document of blocks and marks, written as `Html` by
//! `markup.rich_to_html`; and `LocalisedText`, locales mapped to text,
//! read with `text.pick`.
//!
//! `RichText` is Portable Text's flat shape with its default schema:
//! a list of blocks, each a paragraph, a heading, a quote or a list item
//! with its level, holding spans of text, each with its marks and at most
//! one link. Nothing nests, so no type refers to itself: a list item's
//! level says how deep it sits, as Portable Text's `level` does. Every
//! block and mark is an element [`super::markup::ELEMENTS`] keeps, and a
//! link only an address the sanitiser keeps ([`LINK_PATTERN`]), so what
//! [`render`] writes is already a fixed point of the sanitiser: making it
//! safe changes nothing.
//!
//! `LocalisedText` is a map from `Locale` to text, sent as JSON-LD sends
//! a language map, an object keyed by each locale's tag.

use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, Part, StandardType,
    Storage, TextForm,
};
use crate::Builtin;
use crate::read::SqlType;

/// `RichText`.
pub const RICH_TEXT: &str = "RichText";
/// `RichTextBlock`.
pub const RICH_TEXT_BLOCK: &str = "RichTextBlock";
/// `RichTextSpan`.
pub const RICH_TEXT_SPAN: &str = "RichTextSpan";
/// `RichTextMark`.
pub const RICH_TEXT_MARK: &str = "RichTextMark";
/// `LocalisedText`.
pub const LOCALISED_TEXT: &str = "LocalisedText";

/// The deepest a list item sits: nine levels, as Word and Google Docs
/// nest lists.
pub const LIST_LEVELS: i64 = 9;

/// A link's address (chunk 5.5o2): an `https:` URL with a host, or a
/// `mailto:` address, written so the sanitiser keeps it as it is. The host
/// is ASCII labels of letters, digits and single hyphens, the last
/// beginning with a letter (so it is never read as an IP address, and no
/// label is punycode); a port is at most 65535; the rest is the characters
/// a URL writes unescaped. Anything else a link might be (`http:`, a
/// relative address, an internationalised host written in Unicode) is
/// refused rather than kept as a link the sanitiser would drop.
pub const LINK_PATTERN: &str = concat!(
    r"(?:https://",
    r"(?:[A-Za-z0-9]+(?:-[A-Za-z0-9]+)*\.)*[A-Za-z][A-Za-z0-9]*(?:-[A-Za-z0-9]+)*",
    r"(?::(?:[0-9]{1,4}|[1-5][0-9]{4}|6[0-4][0-9]{3}|65[0-4][0-9]{2}|655[0-2][0-9]|6553[0-5]))?",
    r"(?:[/?#][A-Za-z0-9._~!$&'()*+,;=:@/?#%-]*)?",
    r"|mailto:[A-Za-z0-9.!#$%&'*+/=?^_{|}~-]+@",
    r"(?:[A-Za-z0-9]+(?:-[A-Za-z0-9]+)*\.)*[A-Za-z][A-Za-z0-9]*(?:-[A-Za-z0-9]+)*",
    r"(?:\?[A-Za-z0-9._~!$&'()*+,;=:@/?%-]*)?)",
);

/// `RichTextMark`'s variants, each its own code: Portable Text's default
/// decorators, in the order they nest, outermost first.
pub const RICH_TEXT_MARKS: &[(&str, &str)] = &[
    ("strong", "strong"),
    ("emphasis", "emphasis"),
    ("code", "code"),
    ("underline", "underline"),
    ("strike_through", "strike_through"),
];

/// The element each mark is written as, in [`RICH_TEXT_MARKS`]' order.
pub const MARK_ELEMENTS: &[&str] = &["strong", "em", "code", "u", "s"];

/// A block's spans.
const SPANS: Part = Part::List(&Part::Standard(RICH_TEXT_SPAN));

/// `RichTextBlock`'s variants: Portable Text's default styles (a
/// paragraph, a heading of levels 1 to 6, a quote) and its two kinds of
/// list item, each with its level.
pub const RICH_TEXT_BLOCKS: &[(&str, &[(&str, Part)])] = &[
    ("paragraph", &[("spans", SPANS)]),
    ("heading", &[("level", Part::Whole(1, 6)), ("spans", SPANS)]),
    ("quote", &[("spans", SPANS)]),
    (
        "bullet_item",
        &[("level", Part::Whole(1, LIST_LEVELS)), ("spans", SPANS)],
    ),
    (
        "numbered_item",
        &[("level", Part::Whole(1, LIST_LEVELS)), ("spans", SPANS)],
    ),
];

/// `RichTextSpan`'s fields, in order: its text, which may run over lines,
/// its marks, and the address it links to, if any (a span links to one
/// address at most, as an HTML link can't hold another).
pub const RICH_TEXT_SPAN_FIELDS: [(&str, Part); 3] = [
    ("text", Part::Paragraph),
    ("marks", Part::List(&Part::Standard(RICH_TEXT_MARK))),
    ("link", Part::Optional(&Part::Matching(LINK_PATTERN))),
];

const fn examples(
    valid: &'static [(&'static str, &'static str)],
    malformed: &'static [&'static str],
) -> Examples {
    Examples {
        valid,
        format: None,
        malformed,
        of: None,
        settings: "",
    }
}

/// A document: its blocks, sent as a JSON array of them, stored as that
/// JSON, and shown as the text people read in it.
pub const RICH_TEXT_ENTRY: StandardType = StandardType {
    name: RICH_TEXT,
    holds: Holds::List(Part::Standard(RICH_TEXT_BLOCK)),
    text: None,
    json: Json::Array,
    storage: Storage::Column(SqlType::Json),
    format: Format {
        key: FormatKey::PlainText,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"[{"tag":"heading","level":2,"spans":[{"text":"Soups","marks":[]}]},{"tag":"paragraph","spans":[{"text":"Made ","marks":[]},{"text":"daily","marks":["strong"]}]}]"#,
                "Soups\nMade daily",
            ),
            (
                r#"[{"tag":"bullet_item","level":1,"spans":[{"text":"Leek","marks":[]}]},{"tag":"bullet_item","level":1,"spans":[{"text":"Tomato","marks":[]}]}]"#,
                "Leek\nTomato",
            ),
            (r#"[]"#, ""),
        ],
        &[
            r#"[{"tag":"heading","level":7,"spans":[]}]"#,
            r#"[{"tag":"table","spans":[]}]"#,
            r#"[{"tag":"paragraph","spans":[{"text":"x","marks":["blink"]}]}]"#,
            r#"[{"tag":"paragraph","spans":[{"text":"x","marks":[],"link":"javascript:alert(1)"}]}]"#,
            r#"{"tag":"paragraph","spans":[]}"#,
            r#""<p>Soups</p>""#,
        ],
    ),
    release: FIRST_RELEASE,
};

/// One block of a document: a choice, sent tagged, stored as that JSON and
/// shown as the text people read in it.
pub const RICH_TEXT_BLOCK_ENTRY: StandardType = StandardType {
    name: RICH_TEXT_BLOCK,
    holds: Holds::Choice(RICH_TEXT_BLOCKS),
    text: None,
    json: Json::Tagged,
    storage: Storage::Column(SqlType::Json),
    format: Format {
        key: FormatKey::PlainText,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"tag":"paragraph","spans":[{"text":"Fresh ","marks":[]},{"text":"soup","marks":["emphasis"]}]}"#,
                "Fresh soup",
            ),
            (
                r#"{"tag":"heading","level":1,"spans":[{"text":"Menu","marks":[]}]}"#,
                "Menu",
            ),
            (
                r#"{"tag":"quote","spans":[{"text":"Best in town","marks":[]}]}"#,
                "Best in town",
            ),
            (
                r#"{"tag":"numbered_item","level":2,"spans":[{"text":"Stir","marks":[]}]}"#,
                "Stir",
            ),
        ],
        &[
            r#"{"tag":"heading","level":0,"spans":[]}"#,
            r#"{"tag":"bullet_item","level":10,"spans":[]}"#,
            r#"{"tag":"paragraph"}"#,
            r#"{"tag":"code_block","spans":[]}"#,
            r#""paragraph""#,
        ],
    ),
    release: FIRST_RELEASE,
};

/// A run of text with its marks and link: sent as its members, the link
/// left out when there is none, stored as that JSON and shown as its text.
pub const RICH_TEXT_SPAN_ENTRY: StandardType = StandardType {
    name: RICH_TEXT_SPAN,
    holds: Holds::Object(&RICH_TEXT_SPAN_FIELDS),
    text: None,
    json: Json::Members(&["text", "marks", "link"]),
    storage: Storage::Column(SqlType::Json),
    format: Format {
        key: FormatKey::PlainText,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (r#"{"text":"Soup","marks":[]}"#, "Soup"),
            (
                r#"{"text":"today's menu","marks":["strong","emphasis"]}"#,
                "today's menu",
            ),
            (
                r#"{"text":"our site","marks":[],"link":"https://example.com/menu?day=mon#soups"}"#,
                "our site",
            ),
            (
                r#"{"text":"write to us","marks":["underline"],"link":"mailto:hello@example.com"}"#,
                "write to us",
            ),
        ],
        &[
            r#"{"text":"","marks":[]}"#,
            r#"{"text":"Soup"}"#,
            r#"{"text":"a","marks":[],"link":"http://example.com/"}"#,
            r#"{"text":"a","marks":[],"link":"/menu"}"#,
            r#"{"text":"a","marks":[],"link":"//example.com/"}"#,
            r#"{"text":"a","marks":[],"link":"https://exa mple.com/"}"#,
            r#"{"text":"a","marks":[],"link":"https://192.168.0.1/"}"#,
            r#"{"text":"a","marks":[],"link":"https://example.com:99999/"}"#,
            r#"{"text":"ring\u0007","marks":[]}"#,
            r#""Soup""#,
        ],
    ),
    release: FIRST_RELEASE,
};

/// A mark on a span of text, each its own code.
pub const RICH_TEXT_MARK_ENTRY: StandardType = StandardType {
    name: RICH_TEXT_MARK,
    holds: Holds::Enum(RICH_TEXT_MARKS),
    text: Some(TextForm::Code),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: Format {
        key: FormatKey::Canonical,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("strong", "strong"),
            ("emphasis", "emphasis"),
            ("code", "code"),
            ("underline", "underline"),
            ("strike_through", "strike_through"),
        ],
        &["bold", "em", "strike-through", ""],
    ),
    release: FIRST_RELEASE,
};

/// Text in several locales: sent as an object keyed by each locale's tag,
/// stored as that JSON, and shown as the entry `text.pick` gives for the
/// locale it is shown in.
pub const LOCALISED_TEXT_ENTRY: StandardType = StandardType {
    name: LOCALISED_TEXT,
    holds: Holds::Map(Part::Standard(crate::LOCALE), Part::Of(Builtin::Text)),
    text: None,
    json: Json::Keyed,
    storage: Storage::Column(SqlType::Json),
    format: Format {
        key: FormatKey::Picked,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"en-GB":"Rye loaf","fr-FR":"Pain de seigle"}"#,
                "Rye loaf",
            ),
            (r#"{"en-US":"Rye bread"}"#, "Rye bread"),
            (r#"{"de-DE":"Roggenbrot"}"#, "Roggenbrot"),
            (r#"{}"#, ""),
        ],
        &[
            r#"{"english":"Rye loaf"}"#,
            r#"{"en-GB":42}"#,
            r#"[{"locale":"en-GB","text":"Rye loaf"}]"#,
            r#""Rye loaf""#,
        ],
    ),
    release: FIRST_RELEASE,
};

// ---------------------------------------------------------------------
// Writing a document as HTML
// ---------------------------------------------------------------------

/// A block of a document, as [`render`] reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Paragraph(Vec<Span>),
    /// A heading of level 1 to 6.
    Heading(u8, Vec<Span>),
    Quote(Vec<Span>),
    /// A list item: numbered or not, at its level from 1.
    Item {
        numbered: bool,
        level: u8,
        spans: Vec<Span>,
    },
}

/// A span of a document, as [`render`] reads it: its text, its marks by
/// their place in [`RICH_TEXT_MARKS`], and its link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub marks: Vec<usize>,
    pub link: Option<String>,
}

/// `text` escaped as the sanitiser's serialiser escapes text, so the
/// sanitiser gives it back unchanged.
fn escape_text(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\u{a0}' => out.push_str("&nbsp;"),
            '\n' => out.push_str("<br>"),
            c => out.push(c),
        }
    }
}

/// `text` escaped as the serialiser escapes a quoted attribute's value.
fn escape_attribute(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\u{a0}' => out.push_str("&nbsp;"),
            c => out.push(c),
        }
    }
}

fn render_spans(spans: &[Span], out: &mut String) {
    for span in spans {
        // Each mark once, in their nesting order, whatever order the span
        // lists them in; the link outside them all.
        let marks: Vec<&str> = MARK_ELEMENTS
            .iter()
            .enumerate()
            .filter(|(at, _)| span.marks.contains(at))
            .map(|(_, element)| *element)
            .collect();
        if let Some(link) = &span.link {
            out.push_str("<a href=\"");
            escape_attribute(link, out);
            out.push_str("\" rel=\"");
            out.push_str(super::markup::LINK_REL);
            out.push_str("\">");
        }
        for element in &marks {
            out.push('<');
            out.push_str(element);
            out.push('>');
        }
        escape_text(&span.text, out);
        for element in marks.iter().rev() {
            out.push_str("</");
            out.push_str(element);
            out.push('>');
        }
        if span.link.is_some() {
            out.push_str("</a>");
        }
    }
}

/// The document as HTML, with nothing the sanitiser would change: each
/// block its element, each list item in a list of its kind, a deeper item
/// in a list inside the item before it, a line break in a span's text a
/// `<br>`. `markup.rich_to_html` sanitises it all the same, so an `Html`
/// value is only ever what the sanitiser keeps.
pub fn render(blocks: &[Block]) -> String {
    let mut out = String::new();
    // The lists open around the item being written: each its kind and the
    // level of its items.
    let mut open: Vec<(bool, u8)> = Vec::new();
    for block in blocks {
        let (numbered, level, spans) = match block {
            Block::Item {
                numbered,
                level,
                spans,
            } => (*numbered, *level, spans),
            Block::Paragraph(spans) => {
                close_lists(&mut open, 0, &mut out);
                wrap("p", spans, &mut out);
                continue;
            }
            Block::Heading(level, spans) => {
                close_lists(&mut open, 0, &mut out);
                wrap(&format!("h{level}"), spans, &mut out);
                continue;
            }
            Block::Quote(spans) => {
                close_lists(&mut open, 0, &mut out);
                wrap("blockquote", spans, &mut out);
                continue;
            }
        };
        close_lists(&mut open, level, &mut out);
        match open.last().copied() {
            // A sibling in the same list.
            Some((kind, at)) if at == level && kind == numbered => out.push_str("</li><li>"),
            // A sibling of the other kind is a list of its own; an item
            // deeper than the one before, or the first, a new list, inside
            // the item before when there is one.
            last => {
                if last.is_some_and(|(_, at)| at == level) {
                    close_lists(&mut open, level - 1, &mut out);
                }
                out.push_str(if numbered { "<ol><li>" } else { "<ul><li>" });
                open.push((numbered, level));
            }
        }
        render_spans(spans, &mut out);
    }
    close_lists(&mut open, 0, &mut out);
    out
}

/// The spans in the element `element`.
fn wrap(element: &str, spans: &[Span], out: &mut String) {
    out.push('<');
    out.push_str(element);
    out.push('>');
    render_spans(spans, out);
    out.push_str("</");
    out.push_str(element);
    out.push('>');
}

/// Closes the open lists whose items are deeper than `level`, each with
/// its last item.
fn close_lists(open: &mut Vec<(bool, u8)>, level: u8, out: &mut String) {
    while let Some(&(numbered, at)) = open.last()
        && at > level
    {
        open.pop();
        out.push_str(if numbered { "</li></ol>" } else { "</li></ul>" });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::standard::markup::sanitise;

    fn span(text: &str) -> Span {
        Span {
            text: text.to_string(),
            marks: Vec::new(),
            link: None,
        }
    }

    fn marked(text: &str, marks: &[usize], link: Option<&str>) -> Span {
        Span {
            text: text.to_string(),
            marks: marks.to_vec(),
            link: link.map(str::to_string),
        }
    }

    fn item(numbered: bool, level: u8, text: &str) -> Block {
        Block::Item {
            numbered,
            level,
            spans: vec![span(text)],
        }
    }

    /// What `render` writes is what the sanitiser keeps: sanitising it
    /// changes nothing.
    fn kept(blocks: &[Block]) -> String {
        let html = render(blocks);
        assert_eq!(sanitise(&html), html, "the sanitiser keeps it all");
        html
    }

    #[test]
    fn each_block_is_its_element() {
        assert_eq!(
            kept(&[
                Block::Heading(2, vec![span("Soups")]),
                Block::Paragraph(vec![span("Made "), marked("daily", &[0], None)]),
                Block::Quote(vec![span("Best in town")]),
            ]),
            "<h2>Soups</h2><p>Made <strong>daily</strong></p><blockquote>Best in town</blockquote>"
        );
        assert_eq!(kept(&[]), "");
        assert_eq!(kept(&[Block::Paragraph(Vec::new())]), "<p></p>");
    }

    #[test]
    fn marks_nest_in_one_order_inside_the_link() {
        assert_eq!(
            kept(&[Block::Paragraph(vec![marked(
                "menu",
                &[4, 0, 2, 1, 3, 0],
                Some("https://example.com/menu?a=1&b=2#soups"),
            )])]),
            "<p><a href=\"https://example.com/menu?a=1&amp;b=2#soups\" rel=\"noopener noreferrer nofollow\"><strong><em><code><u><s>menu</s></u></code></em></strong></a></p>"
        );
        assert_eq!(
            kept(&[Block::Paragraph(vec![marked(
                "mail",
                &[],
                Some("mailto:hello@example.com?subject=Soup"),
            )])]),
            "<p><a href=\"mailto:hello@example.com?subject=Soup\" rel=\"noopener noreferrer nofollow\">mail</a></p>"
        );
    }

    #[test]
    fn text_is_escaped_and_never_markup() {
        assert_eq!(
            kept(&[Block::Paragraph(vec![span(
                "<script>alert(1)</script> & \"quotes\" 'too'\u{a0}x"
            )])]),
            "<p>&lt;script&gt;alert(1)&lt;/script&gt; &amp; \"quotes\" 'too'&nbsp;x</p>"
        );
        assert_eq!(
            kept(&[Block::Paragraph(vec![span("one\ntwo\n")])]),
            "<p>one<br>two<br></p>"
        );
    }

    #[test]
    fn list_items_make_lists_by_kind_and_level() {
        assert_eq!(
            kept(&[
                item(false, 1, "Leek"),
                item(false, 1, "Tomato"),
                item(true, 2, "Chop"),
                item(true, 2, "Stir"),
                item(false, 1, "Pea"),
                item(true, 1, "One"),
                Block::Paragraph(vec![span("After")]),
                item(false, 3, "Deep"),
                item(false, 1, "Up"),
            ]),
            concat!(
                "<ul><li>Leek</li><li>Tomato<ol><li>Chop</li><li>Stir</li></ol></li><li>Pea</li></ul>",
                "<ol><li>One</li></ol>",
                "<p>After</p>",
                "<ul><li>Deep</li></ul><ul><li>Up</li></ul>",
            )
        );
        // Nine levels deep, each inside the last.
        let deep: Vec<Block> = (1..=9).map(|level| item(true, level, "x")).collect();
        let html = kept(&deep);
        assert_eq!(html.matches("<ol>").count(), 9);
    }

    /// Links made at random from the pattern's own pieces, and strings of
    /// every character a link might hold: whatever the pattern accepts,
    /// the sanitiser keeps as it is written.
    #[test]
    fn no_link_the_pattern_accepts_is_dropped() {
        let accepts = |link: &str| {
            crate::Rules {
                pattern: Some(LINK_PATTERN.to_string()),
                ..crate::Rules::default()
            }
            .check_text(link)
            .is_ok()
        };
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = |below: usize| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % below as u64) as usize
        };
        // Characters a link may hold, and some it may not, a few of them
        // more often than the rest so hosts and ports come out whole.
        let allowed: Vec<char> = "abcXYZ019-._~!$&'()*+,;=:@/?#%{|}^".chars().collect();
        let refused: Vec<char> = "\" <>\\[]`\u{e9}\u{a0}\t".chars().collect();
        let labels = [
            "a", "example", "x1", "b-c", "xn--zz", "com", "1", "Z9", "-", "",
        ];
        let ports = ["", ":443", ":0", ":65535", ":65536", ":99999", ":", ":08"];
        let mut accepted = 0;
        for _ in 0..4_000 {
            let mut host = String::new();
            for at in 0..=next(3) {
                if at > 0 {
                    host.push('.');
                }
                host.push_str(labels[next(labels.len())]);
            }
            let mut rest = String::new();
            for _ in 0..next(10) {
                rest.push(if next(8) == 0 {
                    refused[next(refused.len())]
                } else {
                    allowed[next(allowed.len())]
                });
            }
            let link = if next(2) == 0 {
                let start = ["/", "?", "#", ""][next(4)];
                format!("https://{host}{}{start}{rest}", ports[next(ports.len())])
            } else {
                let local: String = rest.chars().filter(|c| *c != '@').collect();
                format!(
                    "mailto:{local}@{host}{}",
                    ["", "?subject=Hi", "?a=%20&b"][next(3)]
                )
            };
            if accepts(&link) {
                accepted += 1;
                let html = render(&[Block::Paragraph(vec![marked("x", &[], Some(&link))])]);
                assert_eq!(sanitise(&html), html, "`{link}` is kept as written");
            }
        }
        assert!(
            accepted > 300,
            "enough links were made to tell ({accepted})"
        );
    }

    #[test]
    fn every_link_the_pattern_accepts_is_kept_as_written() {
        let accepts = |link: &str| {
            crate::Rules {
                pattern: Some(LINK_PATTERN.to_string()),
                ..crate::Rules::default()
            }
            .check_text(link)
            .is_ok()
        };
        let links = [
            "https://example.com",
            "https://example.com/",
            "https://EXAMPLE.com/Menu",
            "https://a.b-c.example/x/y?z=1&w=%20#frag",
            "https://example.com:443/",
            "https://example.com:65535",
            "https://intranet",
            "https://x1.example/~me/(a)!$'*+,;=:@",
            "https://example.com/%zz",
            "https://example.com?",
            "https://example.com#",
            "mailto:hello@example.com",
            "mailto:first.last+tag@sub.example.co.uk",
            "mailto:a!#$%&'*+/=?^_{|}~-b@example.com?subject=Hi&body=x%20y",
        ];
        for link in links {
            assert!(accepts(link), "`{link}` is a link");
            kept(&[Block::Paragraph(vec![marked("x", &[], Some(link))])]);
        }
        for link in [
            "http://example.com/",
            "HTTPS://example.com/",
            "https://",
            "https://example..com/",
            "https://-example.com/",
            "https://example-.com/",
            "https://xn--zz.com/",
            "https://1.2.3.4/",
            "https://example.123/",
            "https://example.com:65536/",
            "https://example.com/a b",
            "https://example.com/\"onmouseover=\"x",
            "https://example.com/<x>",
            "https://exämple.com/",
            "https://user@example.com/",
            "javascript:alert(1)",
            "mailto:",
            "mailto:@example.com",
            "mailto:a@",
            "/menu",
            "//example.com/",
            "",
        ] {
            assert!(!accepts(link), "`{link}` isn't a link");
        }
    }
}
