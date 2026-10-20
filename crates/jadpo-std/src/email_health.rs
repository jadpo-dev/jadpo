//! The mail health score (area 16, "A mail health score"; chunk 7.3c2):
//! Jadpo's own checks of a mail for the mistakes that most often send
//! mail to spam or spoil it in a reader's client, each with how to fix
//! it. The dev inbox and `jadpo email show` run [`check_message`] on each
//! mail as it was rendered; `jadpo check` runs the same pieces on the
//! templates as written, wherever their words are known before they are
//! sent.
//!
//! It is an early indication, never a guarantee ([`DISCLAIMER`]): no check
//! can predict where Gmail, Outlook or any other provider puts a mail.

/// What every screen that shows a score says with it.
pub const DISCLAIMER: &str = "An early indication only: it can't predict whether Gmail, Outlook or any other provider puts a mail in the inbox, since each uses its own filters and the sender's history. A clean score means the common mistakes are absent, nothing more.";

/// The size of HTML past which Gmail clips a mail, hiding the rest
/// behind a link: 102 KB.
pub const CLIPPED_AT: usize = 102 * 1024;

/// Fewer characters of text than this beside an image make a mail
/// image-only.
pub const IMAGE_ONLY_TEXT: usize = 20;

/// Shared link shorteners, whose domains spam has put on block lists: a
/// link through one, or a subdomain of one, is flagged.
pub const SHORTENERS: [&str; 22] = [
    "bit.ly",
    "bitly.com",
    "bit.do",
    "buff.ly",
    "cutt.ly",
    "goo.gl",
    "is.gd",
    "lnkd.in",
    "ow.ly",
    "rb.gy",
    "rebrand.ly",
    "s.id",
    "short.io",
    "shorturl.at",
    "t.co",
    "t.ly",
    "tiny.cc",
    "tinyurl.com",
    "tr.im",
    "v.gd",
    "x.co",
    "adf.ly",
];

/// One of Jadpo's checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Check {
    /// The mail has HTML and no plain-text part.
    NoText,
    /// Its HTML is images with almost no text.
    ImageOnly,
    /// Its HTML doesn't parse cleanly: an element closed out of order,
    /// one never closed, or a tag never finished.
    BrokenHtml,
    /// A link that isn't an absolute `https://` address.
    LinkNotHttps,
    /// A link through a shared shortener.
    LinkShortener,
    /// A subject written all in capitals.
    CapitalSubject,
    /// HTML over [`CLIPPED_AT`].
    TooLarge,
}

impl Check {
    /// Every check, in the order a score lists them.
    pub const ALL: [Check; 7] = [
        Check::NoText,
        Check::ImageOnly,
        Check::BrokenHtml,
        Check::LinkNotHttps,
        Check::LinkShortener,
        Check::CapitalSubject,
        Check::TooLarge,
    ];

    /// How `--json` names it.
    pub fn id(self) -> &'static str {
        match self {
            Check::NoText => "no_text_part",
            Check::ImageOnly => "image_only",
            Check::BrokenHtml => "broken_html",
            Check::LinkNotHttps => "link_not_https",
            Check::LinkShortener => "link_shortener",
            Check::CapitalSubject => "capital_subject",
            Check::TooLarge => "too_large",
        }
    }

    /// What it found, as a heading: sentence case, no full stop.
    pub fn title(self) -> &'static str {
        match self {
            Check::NoText => "No plain-text part",
            Check::ImageOnly => "Images with almost no text",
            Check::BrokenHtml => "HTML that doesn't parse cleanly",
            Check::LinkNotHttps => "A link that isn't absolute HTTPS",
            Check::LinkShortener => "A link through a shortener",
            Check::CapitalSubject => "A subject in capitals",
            Check::TooLarge => "HTML Gmail clips",
        }
    }

    /// Why it matters and how to fix it, as one imperative sentence.
    pub fn fix(self) -> &'static str {
        match self {
            Check::NoText => {
                "Add a `.txt` template beside the email's type: filters score HTML-only mail as spam, and some readers show only text"
            }
            Check::ImageOnly => {
                "Write the message as text in the HTML, keeping images to support it: filters can't read images, and many clients block them"
            }
            Check::BrokenHtml => {
                "Close each element, in the reverse order it was opened: clients repair broken HTML differently, and filters score it"
            }
            Check::LinkNotHttps => {
                "Link to a full `https://` address: a mail has no page to resolve a relative link against, and filters distrust plain `http://`"
            }
            Check::LinkShortener => {
                "Link to your own domain directly: shared shorteners hide where a link goes, and their domains are on block lists"
            }
            Check::CapitalSubject => {
                "Write the subject in sentence case: filters score a subject in capitals as shouting"
            }
            Check::TooLarge => {
                "Keep the HTML under 102 KB: Gmail hides the rest behind a link, often the footer and any unsubscribe link"
            }
        }
    }
}

/// What one check found in a mail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub check: Check,
    /// What it found, in a few words: the link, the element, the size.
    pub detail: String,
}

/// Every check a rendered mail fails, in [`Check::ALL`]'s order. A link to
/// this machine (`localhost`) is let through, since a development
/// session's links lead there.
pub fn check_message(subject: &str, html: Option<&str>, text: Option<&str>) -> Vec<Finding> {
    let mut out = Vec::new();
    if let Some(html) = html {
        if text.is_none() {
            out.push(Finding {
                check: Check::NoText,
                detail: "It has HTML only.".to_string(),
            });
        }
        if let Some(detail) = image_only(html) {
            out.push(Finding {
                check: Check::ImageOnly,
                detail,
            });
        }
        if let Some(detail) = html_problem(html) {
            out.push(Finding {
                check: Check::BrokenHtml,
                detail,
            });
        }
    }
    let mut links: Vec<String> = Vec::new();
    for link in html
        .map(html_links)
        .unwrap_or_default()
        .into_iter()
        .chain(text.map(text_links).unwrap_or_default())
    {
        if !links.contains(&link) {
            links.push(link);
        }
    }
    for check in [Check::LinkNotHttps, Check::LinkShortener] {
        let found: Vec<&String> = links
            .iter()
            .filter(|link| link_problem(link, true) == Some(check))
            .collect();
        if let Some(first) = found.first() {
            out.push(Finding {
                check,
                detail: match found.len() {
                    1 => format!("`{first}`"),
                    2 => format!("`{first}` and 1 more"),
                    n => format!("`{first}` and {} more", n - 1),
                },
            });
        }
    }
    if capitals(subject) {
        out.push(Finding {
            check: Check::CapitalSubject,
            detail: format!("\u{201c}{subject}\u{201d}"),
        });
    }
    if let Some(html) = html
        && html.len() > CLIPPED_AT
    {
        out.push(Finding {
            check: Check::TooLarge,
            detail: format!(
                "Its HTML is {} KB; Gmail clips past 102 KB.",
                html.len().div_ceil(1024)
            ),
        });
    }
    out
}

/// Whether `subject` is written all in capitals: four letters or more
/// that have a case, none of them lower case.
pub fn capitals(subject: &str) -> bool {
    let cased: Vec<char> = subject
        .chars()
        .filter(|c| c.is_uppercase() || c.is_lowercase())
        .collect();
    cased.len() >= 4 && cased.iter().all(|c| c.is_uppercase())
}

/// Which link check `link` fails, if any: an address that isn't
/// absolute `https://` fails [`Check::LinkNotHttps`], and one through a
/// shortener [`Check::LinkShortener`]. `mailto:`, `tel:` and `sms:` links
/// pass, and with `local` so does a link to this machine.
pub fn link_problem(link: &str, local: bool) -> Option<Check> {
    let lower = link.trim().to_ascii_lowercase();
    if ["mailto:", "tel:", "sms:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
    {
        return None;
    }
    let (secure, rest) = if let Some(rest) = lower.strip_prefix("https://") {
        (true, rest)
    } else if let Some(rest) = lower.strip_prefix("http://") {
        (false, rest)
    } else {
        return Some(Check::LinkNotHttps);
    };
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default();
    let host = if authority.starts_with('[') {
        authority
            .split(']')
            .next()
            .map(|host| format!("{host}]"))
            .unwrap_or_default()
    } else {
        authority.split(':').next().unwrap_or_default().to_string()
    };
    if host.is_empty() {
        return Some(Check::LinkNotHttps);
    }
    if local && is_local(&host) {
        return None;
    }
    if SHORTENERS
        .iter()
        .any(|short| host == *short || host.ends_with(&format!(".{short}")))
    {
        return Some(Check::LinkShortener);
    }
    (!secure).then_some(Check::LinkNotHttps)
}

/// Whether `host` is this machine.
fn is_local(host: &str) -> bool {
    host == "localhost"
        || host.ends_with(".localhost")
        || host == "[::1]"
        || host.starts_with("127.")
}

/// Why `html` is image-only, when it is: it has an image and fewer than
/// [`IMAGE_ONLY_TEXT`] characters of text.
pub fn image_only(html: &str) -> Option<String> {
    let images = images(html);
    if images == 0 {
        return None;
    }
    let text = visible_text(html)
        .chars()
        .filter(|c| !c.is_whitespace())
        .count();
    (text < IMAGE_ONLY_TEXT).then(|| {
        format!(
            "{} beside {} of text.",
            plural(images, "image"),
            plural(text, "character")
        )
    })
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// How many `<img>` elements `html` has.
pub fn images(html: &str) -> usize {
    tokens(html)
        .iter()
        .filter(|token| matches!(token, Token::Start { name, .. } if name == "img"))
        .count()
}

/// The text a reader sees in `html`: its tags, comments, and the insides
/// of `<head>`, `<style>`, `<script>` and `<title>` left out, `&nbsp;`
/// read as a space.
pub fn visible_text(html: &str) -> String {
    let mut out = String::new();
    let mut hidden: Vec<String> = Vec::new();
    for token in tokens(html) {
        match token {
            Token::Text(text) if hidden.is_empty() => {
                out.push_str(&text.replace("&nbsp;", " "));
            }
            Token::Start {
                name, self_closing, ..
            } if !self_closing && is_hidden(&name) => hidden.push(name),
            Token::End { name, .. } if hidden.last() == Some(&name) => {
                hidden.pop();
            }
            _ => {}
        }
    }
    out
}

fn is_hidden(name: &str) -> bool {
    matches!(name, "head" | "style" | "script" | "title")
}

/// Elements that have no end tag.
const VOID: [&str; 15] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "keygen", "link", "meta", "param",
    "source", "track", "wbr",
];

/// Elements whose end tag HTML lets a writer leave out.
const OPTIONAL_END: [&str; 18] = [
    "html", "head", "body", "p", "li", "dt", "dd", "option", "optgroup", "tr", "td", "th", "thead",
    "tbody", "tfoot", "colgroup", "rt", "rp",
];

/// What is wrong with `html`'s structure, the first problem found, when
/// something is: an end tag that closes nothing or closes another
/// element, an element never closed, or a tag never finished. Elements
/// whose end tag HTML lets a writer leave out (`<p>`, `<li>`, `<td>`) may
/// be left open.
pub fn html_problem(html: &str) -> Option<String> {
    let mut open: Vec<(String, usize)> = Vec::new();
    for token in tokens(html) {
        match token {
            Token::Unfinished { name, line } => {
                return Some(format!("`<{name}` at line {line} is never finished."));
            }
            Token::Start {
                name,
                line,
                self_closing,
            } => {
                if !self_closing && !VOID.contains(&name.as_str()) {
                    open.push((name, line));
                }
            }
            Token::End { name, line } => {
                if VOID.contains(&name.as_str()) {
                    continue;
                }
                let Some(at) = open.iter().rposition(|(open, _)| *open == name) else {
                    return Some(format!("`</{name}>` at line {line} closes nothing."));
                };
                // Everything opened since must be an element that may be
                // left open.
                if let Some((inner, opened)) = open
                    .get(at + 1..)
                    .unwrap_or_default()
                    .iter()
                    .rev()
                    .find(|(inner, _)| !OPTIONAL_END.contains(&inner.as_str()))
                {
                    return Some(format!(
                        "`</{name}>` at line {line} closes `<{inner}>`, opened at line {opened}, first."
                    ));
                }
                open.truncate(at);
            }
            Token::Text(_) | Token::Other => {}
        }
    }
    open.iter()
        .find(|(name, _)| !OPTIONAL_END.contains(&name.as_str()))
        .map(|(name, line)| format!("`<{name}>` at line {line} is never closed."))
}

/// A piece of HTML, as [`tokens`] reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Text(String),
    /// A start tag, its name in lower case.
    Start {
        name: String,
        line: usize,
        self_closing: bool,
    },
    End {
        name: String,
        line: usize,
    },
    /// A tag that never reaches its `>`.
    Unfinished {
        name: String,
        line: usize,
    },
    /// A comment, a doctype or a processing instruction.
    Other,
}

/// `html` read as text and tags, leniently: a `<` that starts no tag is
/// text, and the insides of `<script>`, `<style>`, `<title>` and
/// `<textarea>` are text up to their end tag.
fn tokens(html: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut at = 0;
    let mut text = String::new();
    let line_of = |at: usize| {
        html.get(..at)
            .map_or(1, |head| head.matches('\n').count() + 1)
    };
    while let Some(rest) = html.get(at..) {
        let Some(c) = rest.chars().next() else { break };
        if c != '<' {
            text.push(c);
            at += c.len_utf8();
            continue;
        }
        // A comment, a doctype or a processing instruction.
        if rest.starts_with("<!--") {
            flush(&mut out, &mut text);
            at = rest
                .get(4..)
                .and_then(|after| after.find("-->"))
                .map_or(html.len(), |end| at + 4 + end + 3);
            out.push(Token::Other);
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") {
            flush(&mut out, &mut text);
            at = rest.find('>').map_or(html.len(), |end| at + end + 1);
            out.push(Token::Other);
            continue;
        }
        let closing = rest.starts_with("</");
        let name_from = if closing { 2 } else { 1 };
        let name: String = rest
            .get(name_from..)
            .unwrap_or_default()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        let starts_name = rest
            .get(name_from..)
            .and_then(|after| after.chars().next())
            .is_some_and(|c| c.is_ascii_alphabetic());
        if !starts_name {
            text.push('<');
            at += 1;
            continue;
        }
        flush(&mut out, &mut text);
        let line = line_of(at);
        let name = name.to_ascii_lowercase();
        // The tag's end: the first `>` outside a quoted attribute value.
        let mut quote: Option<char> = None;
        let mut end = None;
        let mut previous = ' ';
        for (offset, c) in rest.char_indices().skip(name_from + name.len()) {
            match (quote, c) {
                (Some(open), c) if c == open => quote = None,
                (Some(_), _) => {}
                (None, '"' | '\'') if previous == '=' || previous.is_whitespace() => {
                    quote = Some(c);
                }
                (None, '>') => {
                    end = Some(offset);
                    break;
                }
                (None, '<') => break,
                _ => {}
            }
            if !c.is_whitespace() || quote.is_some() {
                previous = c;
            }
        }
        let Some(end) = end else {
            out.push(Token::Unfinished { name, line });
            break;
        };
        let inside = rest.get(..end).unwrap_or_default();
        at += end + 1;
        if closing {
            out.push(Token::End { name, line });
            continue;
        }
        let self_closing = inside.trim_end().ends_with('/');
        let raw = matches!(name.as_str(), "script" | "style" | "title" | "textarea");
        out.push(Token::Start {
            name: name.clone(),
            line,
            self_closing,
        });
        if raw && !self_closing {
            // Text up to its end tag.
            let close = format!("</{name}");
            let after = html.get(at..).unwrap_or_default();
            let found = after.to_ascii_lowercase().find(&close);
            let stop = found.unwrap_or(after.len());
            out.push(Token::Text(
                after.get(..stop).unwrap_or_default().to_string(),
            ));
            at += stop;
        }
    }
    flush(&mut out, &mut text);
    out
}

fn flush(out: &mut Vec<Token>, text: &mut String) {
    if !text.is_empty() {
        out.push(Token::Text(std::mem::take(text)));
    }
}

/// Every `href` in `html`, its character references read back, in order.
pub fn html_links(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower.get(from..).and_then(|rest| rest.find("href=")) {
        let start = from + at + "href=".len();
        from = start;
        let rest = html.get(start..).unwrap_or_default();
        let (value, used) = match rest.chars().next() {
            Some(quote @ ('"' | '\'')) => match rest.get(1..).and_then(|after| after.find(quote)) {
                Some(end) => (rest.get(1..=end).unwrap_or_default(), end + 2),
                None => continue,
            },
            Some(_) => {
                let end = rest
                    .find(|c: char| c.is_whitespace() || c == '>')
                    .unwrap_or(rest.len());
                (rest.get(..end).unwrap_or_default(), end)
            }
            None => continue,
        };
        from = start + used;
        let link = unescape_html(value.trim());
        if !link.is_empty() {
            out.push(link);
        }
    }
    out
}

/// Every `http://` or `https://` address written in `text`, in order.
pub fn text_links(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = ["https://", "http://"]
        .iter()
        .filter_map(|scheme| text.get(from..).and_then(|rest| rest.find(scheme)))
        .min()
    {
        let start = from + at;
        let end = text
            .get(start..)
            .and_then(|rest| rest.find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"')))
            .map_or(text.len(), |end| start + end);
        let link = text
            .get(start..end)
            .unwrap_or_default()
            .trim_end_matches(['.', ',', ')', ';', ':', '!', '?']);
        if link.len() > "https://".len() {
            out.push(link.to_string());
        }
        from = end.max(start + 1);
    }
    out
}

/// `&amp;`, `&lt;`, `&gt;`, `&quot;` and `&#39;`, as
/// [`crate::email::escape_html`] writes them, read back.
fn unescape_html(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checks(subject: &str, html: Option<&str>, text: Option<&str>) -> Vec<Check> {
        check_message(subject, html, text)
            .into_iter()
            .map(|finding| finding.check)
            .collect()
    }

    #[test]
    fn a_clean_mail_passes_every_check() {
        let html = "<!DOCTYPE html><html><head><title>Hi</title><style>p{}</style></head><body><p>Welcome, Ada. Your notes are ready whenever you are.<p><a href=\"https://notes.example/a?x=1&amp;y=2\">Open</a><br><img src=\"https://notes.example/logo.png\" alt=\"\"></body></html>";
        assert_eq!(
            checks(
                "Welcome, Ada",
                Some(html),
                Some("Open: https://notes.example/a")
            ),
            Vec::<Check>::new()
        );
    }

    #[test]
    fn each_mistake_is_found() {
        assert_eq!(
            checks("Hi there", Some("<p>Hello</p>"), None),
            [Check::NoText]
        );
        assert_eq!(
            checks(
                "Sale",
                Some("<img src=\"https://x.example/a.png\"><img src=\"https://x.example/b.png\">"),
                Some("x")
            ),
            [Check::ImageOnly]
        );
        assert_eq!(
            check_message("Hi there", Some("<div><b>x</div>"), Some("x"))[0].detail,
            "`</div>` at line 1 closes `<b>`, opened at line 1, first."
        );
        assert_eq!(
            check_message("Hi there", Some("<p>x</p>\n</div>"), Some("x"))[0].detail,
            "`</div>` at line 2 closes nothing."
        );
        assert_eq!(
            check_message("Hi there", Some("<table><tr><td>x"), Some("x"))[0].detail,
            "`<table>` at line 1 is never closed."
        );
        assert_eq!(
            check_message("Hi there", Some("<p>x</p><a href=\"/x"), Some("x"))[0].detail,
            "`<a` at line 1 is never finished."
        );
        assert_eq!(
            checks(
                "Hi there",
                Some("<a href=\"/sign-in\">a</a> <a href=\"http://notes.example\">b</a>"),
                Some("x")
            ),
            [Check::LinkNotHttps]
        );
        assert_eq!(
            check_message(
                "Hi there",
                Some("<a href=\"/sign-in\">a</a> <a href=\"http://notes.example\">b</a>"),
                Some("x")
            )[0]
            .detail,
            "`/sign-in` and 1 more"
        );
        assert_eq!(
            checks("Hi there", None, Some("Go: https://bit.ly/abc")),
            [Check::LinkShortener]
        );
        assert_eq!(
            checks("YOUR ORDER", None, Some("x")),
            [Check::CapitalSubject]
        );
        let big = format!("<p>{}</p>", "a".repeat(CLIPPED_AT));
        assert_eq!(checks("Hi there", Some(&big), Some("x")), [Check::TooLarge]);
    }

    #[test]
    fn links_to_this_machine_and_other_schemes_pass_in_development() {
        assert_eq!(
            link_problem("http://localhost:5173/sign-in?t=1", true),
            None
        );
        assert_eq!(
            link_problem("http://localhost:5173/sign-in", false),
            Some(Check::LinkNotHttps)
        );
        assert_eq!(link_problem("http://127.0.0.1/x", true), None);
        assert_eq!(link_problem("http://[::1]:3000/x", true), None);
        assert_eq!(link_problem("mailto:help@notes.example", false), None);
        assert_eq!(
            link_problem("https://notes.bit.ly/x", false),
            Some(Check::LinkShortener)
        );
        assert_eq!(link_problem("https://notbit.ly/x", false), None);
        assert_eq!(link_problem("#top", false), Some(Check::LinkNotHttps));
        assert_eq!(link_problem("https://", false), Some(Check::LinkNotHttps));
    }

    #[test]
    fn subjects_in_capitals_need_four_cased_letters() {
        assert!(capitals("ACT NOW!"));
        assert!(!capitals("NHS"));
        assert!(!capitals("Your NHS letter"));
        assert!(!capitals("123 456"));
    }

    #[test]
    fn optional_end_tags_and_raw_text_are_read_as_html_reads_them() {
        assert_eq!(html_problem("<ul><li>a<li>b</ul><p>c"), None);
        assert_eq!(
            html_problem("<script>if (a < b) { x = '</p>' }</script>"),
            None
        );
        assert_eq!(html_problem("<p>a < b and c > d</p>"), None);
        assert_eq!(html_problem("<a title=\"x > y\" href='z'>a</a>"), None);
        assert_eq!(html_problem("<br/><img src=x /><hr></br>"), None);
        assert_eq!(html_problem("<!-- <div> -->"), None);
    }

    #[test]
    fn every_link_in_html_and_text_is_found() {
        assert_eq!(
            html_links(
                "<a href=\"https://notes.example/sign-in?a=1&amp;b=2\">Sign in</a> <A HREF='https://notes.example/help'>Help</a> <a href=/x>x</a>"
            ),
            vec![
                "https://notes.example/sign-in?a=1&b=2",
                "https://notes.example/help",
                "/x"
            ]
        );
        assert_eq!(
            text_links(
                "Sign in: https://notes.example/sign-in?a=1&b=2.\nOr http://notes.example/help, later"
            ),
            vec![
                "https://notes.example/sign-in?a=1&b=2",
                "http://notes.example/help"
            ]
        );
    }
}
