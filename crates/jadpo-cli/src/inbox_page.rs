//! The dev inbox's page (area 16, "Email"; chunk 7.3c2): `jadpo dev`
//! serves it on its own port on this machine only, beside the app, and it
//! reads the inbox `jadpo dev` keeps ([`crate::dev_inbox`]).
//!
//! Two panes, the CLI design language's tokens carried into the browser:
//! the mail newest first, and the one chosen, its HTML as a reader's
//! client shows it (in a sandboxed frame, its links opening in a new
//! tab), its text, its links and what the health checks found, with
//! SpamAssassin's rules when it was run on the mail (chunk 7.3c3). The page
//! is read-only, as the developer console is (area 17): `jadpo email
//! clear` empties the inbox. It looks for new mail every two seconds.
//!
//! Only a request addressed to this machine by name is answered, so a page
//! elsewhere can't reach the inbox by pointing a name of its own at it.

use std::path::Path;

use jadpo_runtime::http::{Request, Response};
use jadpo_std::email::escape_html as esc;
use jadpo_std::email_health::{Check, DISCLAIMER};

use crate::dev_inbox::{self, Mail};
use crate::spam::Spam;

/// What the page answers `request` with, reading the inbox at `root`.
pub(crate) fn answer(root: &Path, request: &Request) -> Response {
    if !local_host(request.header("host")) {
        return plain(
            421,
            "The inbox answers only requests addressed to localhost.",
        );
    }
    if request.method != "GET" && request.method != "HEAD" {
        let mut response = plain(405, "The inbox is read-only.");
        response.headers.push(("allow".into(), "GET, HEAD".into()));
        return response;
    }
    let view = request
        .query
        .as_deref()
        .and_then(|query| query.split('&').find_map(|pair| pair.strip_prefix("view=")))
        .unwrap_or("");
    let segments: Vec<&str> = request.path.trim_matches('/').split('/').collect();
    match segments.as_slice() {
        [""] => {
            let all = dev_inbox::read_all(root);
            let chosen = all.last().cloned();
            page(&all, chosen.as_ref(), view, true)
        }
        ["inbox.json"] => {
            let all = dev_inbox::read_all(root);
            let body = serde_json::json!({
                "newest": all.last().map(|mail| mail.id.clone()),
                "count": all.len(),
            });
            respond(
                200,
                "application/json",
                format!("{body}\n").into_bytes(),
                Vec::new(),
            )
        }
        ["mail", id] => {
            let all = dev_inbox::read_all(root);
            match all.iter().find(|mail| mail.id == *id) {
                Some(mail) => page(&all, Some(mail), view, false),
                None => {
                    let mut response = page(&all, None, view, false);
                    response.status = 404;
                    response
                }
            }
        }
        ["mail", id, "body"] => match dev_inbox::read(root, id).and_then(|mail| mail.html) {
            Some(html) => respond(
                200,
                "text/html; charset=utf-8",
                framed(&html).into_bytes(),
                vec![(
                    "content-security-policy".into(),
                    "sandbox allow-popups allow-popups-to-escape-sandbox; default-src 'none'; img-src * data:; style-src 'unsafe-inline' *; font-src * data:".into(),
                )],
            ),
            None => plain(404, "This mail has no HTML part."),
        },
        ["mail", id, "attachments", index] => {
            let found = dev_inbox::read(root, id).and_then(|mail| {
                let index = index.parse::<usize>().ok()?;
                let name = mail.attachments.get(index)?.name.clone();
                Some((name, dev_inbox::attachment(root, &mail, index)?))
            });
            match found {
                Some((name, bytes)) => respond(
                    200,
                    "application/octet-stream",
                    bytes,
                    vec![(
                        "content-disposition".into(),
                        format!("attachment; filename=\"{}\"", name.replace(['"', '\\', '\r', '\n'], "_")),
                    )],
                ),
                None => plain(404, "There is no such attachment."),
            }
        }
        _ => plain(404, "There is nothing here."),
    }
}

/// Whether the `Host` header names this machine.
fn local_host(host: Option<&str>) -> bool {
    let Some(host) = host else { return false };
    let name = if host.starts_with('[') {
        host.split(']')
            .next()
            .map(|name| format!("{name}]"))
            .unwrap_or_default()
    } else {
        host.rsplit_once(':')
            .map_or(host, |(name, _)| name)
            .to_string()
    };
    let name = name.to_ascii_lowercase();
    name == "localhost" || name == "127.0.0.1" || name == "[::1]" || name.ends_with(".localhost")
}

fn respond(
    status: u16,
    content_type: &str,
    body: Vec<u8>,
    extra: Vec<(String, String)>,
) -> Response {
    let mut headers = vec![
        ("content-type".to_string(), content_type.to_string()),
        ("cache-control".to_string(), "no-store".to_string()),
        ("x-content-type-options".to_string(), "nosniff".to_string()),
        ("referrer-policy".to_string(), "no-referrer".to_string()),
    ];
    headers.extend(extra);
    Response {
        status,
        headers,
        body,
        ..Response::default()
    }
}

fn plain(status: u16, text: &str) -> Response {
    respond(
        status,
        "text/plain; charset=utf-8",
        format!("{text}\n").into_bytes(),
        Vec::new(),
    )
}

/// A mail's HTML as its frame shows it: its links opening in a new tab,
/// since the frame is sandboxed, on the white a mail client gives it.
fn framed(html: &str) -> String {
    let base = "<base target=\"_blank\">";
    let lower = html.to_ascii_lowercase();
    match lower.find("<head>") {
        Some(at) => format!("{}{base}{}", &html[..at + 6], &html[at + 6..]),
        None => format!("{base}{html}"),
    }
}

/// Which part of a mail the page shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Html,
    Text,
    Links,
    Health,
}

impl View {
    fn word(self) -> &'static str {
        match self {
            View::Html => "html",
            View::Text => "text",
            View::Links => "links",
            View::Health => "health",
        }
    }
}

/// The page: the list, and `chosen` shown in `view` (its HTML when it has
/// one, else its text, when none is asked for).
fn page(all: &[Mail], chosen: Option<&Mail>, view: &str, index: bool) -> Response {
    let mut list = String::new();
    for mail in all.iter().rev() {
        let current = chosen.is_some_and(|chosen| chosen.id == mail.id);
        let problems = mail.problems();
        let health = if problems == 0 {
            "<span class=\"health ok\"><span aria-hidden=\"true\">✓</span><span class=\"hidden\">every health check passes</span></span>".to_string()
        } else {
            format!(
                "<span class=\"health warn\"><span aria-hidden=\"true\">▲ {problems}</span><span class=\"hidden\">{n} health {word}</span></span>",
                n = problems,
                word = if problems == 1 { "problem" } else { "problems" }
            )
        };
        list.push_str(&format!(
            "<li><a href=\"/mail/{id}\"{current}><span class=\"line\"><span class=\"subject\">{subject}</span>{time}</span><span class=\"line\"><span class=\"to\">{to}</span>{health}</span><span class=\"kind\">{kind} · {id}</span></a></li>",
            id = esc(&mail.id),
            current = if current { " aria-current=\"true\"" } else { "" },
            subject = esc(&mail.subject),
            time = time(&mail.at, true),
            to = esc(&mail.to),
            kind = esc(mail.email_short()),
        ));
    }
    let side = if all.is_empty() {
        "<div class=\"empty\"><h2>No mail yet</h2><p>Each mail the app sends while <code>jadpo dev</code> runs lands here, rendered whole. Nothing leaves this machine.</p><p class=\"muted\">An agent reads the same inbox with <code>jadpo email list</code>.</p></div>".to_string()
    } else {
        format!(
            "<ol class=\"mails\">{list}</ol><p class=\"foot\">The last {kept} mails are kept. <code>jadpo email clear</code> empties the inbox. Health checks are an early indication only: they can't predict whether any provider puts a mail in the inbox.</p>",
            kept = dev_inbox::KEPT
        )
    };
    let main = match chosen {
        Some(mail) => detail(mail, view),
        None if all.is_empty() => String::new(),
        None => "<div class=\"missing\"><h1>That mail isn't in the inbox</h1><p>It may have been cleared, or pushed out by newer mail. Choose one from the list.</p></div>".to_string(),
    };
    let newest = all.last().map_or("", |mail| mail.id.as_str());
    let title = match chosen {
        Some(mail) => format!("{} · Inbox", mail.subject),
        None => "Inbox".to_string(),
    };
    let body_class = match (chosen.is_some() && !index, all.is_empty()) {
        (_, true) => "is-empty",
        (true, false) => "has-mail",
        (false, false) => "is-index",
    };
    let count = match all.len() {
        1 => "1 mail".to_string(),
        n => format!("{n} mails"),
    };
    let html = format!(
        "<!doctype html>\n<html lang=\"en-GB\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"color-scheme\" content=\"light dark\"><title>{title} · jadpo dev</title><style>{STYLE}</style></head><body class=\"{body_class}\" data-newest=\"{newest}\"><header class=\"bar\"><a class=\"home\" href=\"/\">Inbox</a><span class=\"facts\">jadpo dev · {count} · nothing leaves this machine</span><span class=\"live\" aria-live=\"polite\"><a id=\"fresh\" class=\"fresh\" href=\"/\" hidden>New mail</a><span id=\"gone\" class=\"gone\" hidden>jadpo dev has stopped; this is the inbox as it was</span></span></header><div class=\"panes\"><nav class=\"side\" aria-label=\"Mail\">{side}</nav><main class=\"main\">{main}</main></div><script>{SCRIPT}</script></body></html>\n",
        title = esc(&title),
        newest = esc(newest),
    );
    respond(
        200,
        "text/html; charset=utf-8",
        html.into_bytes(),
        vec![(
            "content-security-policy".into(),
            "default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; frame-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'".into(),
        )],
    )
}

/// An instant as the page shows it: `<time>` with the instant whole, which
/// the script words in the reader's own zone.
fn time(at: &str, short: bool) -> String {
    let shown = at.replace('T', " ").replace('Z', " UTC");
    format!(
        "<time datetime=\"{at}\"{short}>{shown}</time>",
        at = esc(at),
        short = if short { " data-short" } else { "" },
        shown = esc(&shown),
    )
}

fn detail(mail: &Mail, asked: &str) -> String {
    let links = mail.links();
    let findings = mail.health();
    let problems = mail.problems();
    let view = match asked {
        "html" if mail.html.is_some() => View::Html,
        "text" => View::Text,
        "links" => View::Links,
        "health" => View::Health,
        _ if mail.html.is_some() => View::Html,
        _ => View::Text,
    };
    let id = esc(&mail.id);
    let mut meta = vec![
        ("from", esc(&mail.from)),
        ("to", esc(&mail.to)),
        ("sent", time(&mail.at, false)),
    ];
    if let Some(code) = sign_in_code(mail) {
        meta.push((
            "code",
            format!(
                "<span class=\"code\">{code}</span> <button type=\"button\" data-copy=\"{code}\">Copy code</button>"
            ),
        ));
    }
    meta.push((
        "type",
        format!("<span class=\"muted\">{}</span>", esc(&mail.email)),
    ));
    if let Some(trace) = &mail.trace {
        meta.push((
            "trace",
            format!("<span class=\"muted\">{}</span>", esc(trace)),
        ));
    }
    for (index, attachment) in mail.attachments.iter().enumerate() {
        meta.push((
            "attached",
            format!(
                "<a href=\"/mail/{id}/attachments/{index}\" download>{name}</a> <span class=\"muted\">{kind} · {size}</span>",
                name = esc(&attachment.name),
                kind = esc(&attachment.content_type),
                size = crate::email::size(attachment.size as usize),
            ),
        ));
    }
    let meta: String = meta
        .into_iter()
        .map(|(label, value)| format!("<dt>{label}</dt><dd>{value}</dd>"))
        .collect();
    let total = Check::ALL.len();
    let spam_note = format!(
        "<span class=\"spam-note\">· {}</span>",
        esc(&mail.spam.short())
    );
    let strip = if problems == 0 {
        format!(
            "<p class=\"strip\"><span class=\"sym ok\" aria-hidden=\"true\">✓</span> Health: all {total} checks pass {spam_note}<a href=\"/mail/{id}?view=health\">What was checked</a></p>"
        )
    } else {
        format!(
            "<p class=\"strip\"><span class=\"sym warn\" aria-hidden=\"true\">▲</span> Health: <strong>{n}</strong> · {pass} of {total} checks pass {spam_note}<a href=\"/mail/{id}?view=health\">See what to fix</a></p>",
            n = if problems == 1 {
                "1 problem".to_string()
            } else {
                format!("{problems} problems")
            },
            pass = total - findings.len(),
        )
    };
    let tab = |this: View, label: String| {
        let current = if this == view {
            " aria-current=\"page\""
        } else {
            ""
        };
        format!(
            "<a href=\"/mail/{id}?view={word}\"{current}>{label}</a>",
            word = this.word()
        )
    };
    let mut tabs = String::new();
    if mail.html.is_some() {
        tabs.push_str(&tab(View::Html, "HTML".to_string()));
    }
    tabs.push_str(&tab(View::Text, "Text".to_string()));
    tabs.push_str(&tab(
        View::Links,
        format!("Links <span class=\"count\">{}</span>", links.len()),
    ));
    tabs.push_str(&tab(
        View::Health,
        if problems == 0 {
            "Health checks".to_string()
        } else {
            format!("Health checks <span class=\"count warn\">{problems}</span>")
        },
    ));
    let shown = match view {
        View::Html => format!(
            "<p class=\"note\">As a reader's mail client shows it, on white. Links open in a new tab. <a href=\"/mail/{id}/body\" target=\"_blank\" rel=\"noreferrer\">Open it alone</a></p><iframe class=\"frame\" src=\"/mail/{id}/body\" sandbox=\"allow-popups allow-popups-to-escape-sandbox\" title=\"The mail's HTML\"></iframe>"
        ),
        View::Text => match &mail.text {
            Some(text) => format!("<pre class=\"text\">{}</pre>", linked(text)),
            None => "<div class=\"absent\"><p><strong>This mail has no plain-text part.</strong> Filters score HTML-only mail as spam, and some readers show only text.</p><p class=\"muted\">Add a <code>.txt</code> template beside the email's type.</p></div>".to_string(),
        },
        View::Links => {
            if links.is_empty() {
                "<div class=\"absent\"><p>This mail has no links.</p></div>".to_string()
            } else {
                let items: String = links
                    .iter()
                    .enumerate()
                    .map(|(at, link)| {
                        let web = link.starts_with("https://") || link.starts_with("http://");
                        let shown = if web {
                            format!(
                                "<a href=\"{href}\" target=\"_blank\" rel=\"noreferrer\">{text}</a>",
                                href = esc(link),
                                text = esc(link)
                            )
                        } else {
                            format!("<span>{}</span>", esc(link))
                        };
                        format!(
                            "<li><span class=\"n\">{n}</span><div class=\"link\">{shown}<div class=\"actions\"><button type=\"button\" data-copy=\"{copy}\">Copy link</button><code>jadpo email open {id} {n}</code><button type=\"button\" data-copy=\"jadpo email open {id} {n}\">Copy command</button></div></div></li>",
                            n = at + 1,
                            copy = esc(link),
                        )
                    })
                    .collect();
                format!("<ol class=\"links\">{items}</ol>")
            }
        }
        View::Health => {
            let mut out = String::new();
            for finding in &findings {
                out.push_str(&format!(
                    "<li class=\"warn\"><span class=\"sym\" aria-hidden=\"true\">▲</span><div><h2>{title}</h2><p class=\"found\">{found}</p><p>{fix}</p></div></li>",
                    title = esc(finding.check.title()),
                    found = code_spans(&finding.detail),
                    fix = code_spans(finding.check.fix()),
                ));
            }
            let passed: String = Check::ALL
                .iter()
                .filter(|check| !findings.iter().any(|finding| finding.check == **check))
                .map(|check| {
                    format!(
                        "<li class=\"ok\"><span class=\"sym\" aria-hidden=\"true\">✓</span><div><h2>{}</h2></div></li>",
                        esc(&passed_title(*check))
                    )
                })
                .collect();
            format!(
                "<ul class=\"checks\">{out}{passed}</ul>{}<p class=\"disclaimer\">{}</p>",
                spam_section(&mail.spam),
                esc(DISCLAIMER)
            )
        }
    };
    format!(
        "<a class=\"back\" href=\"/\">← Inbox</a><h1>{subject}</h1><dl class=\"meta\">{meta}</dl>{strip}<p class=\"caveat\">{caveat}</p><nav class=\"tabs\" aria-label=\"Parts of the mail\">{tabs}</nav><div class=\"view view-{word}\">{shown}</div>",
        subject = esc(&mail.subject),
        caveat = esc(
            "The health score is an early indication only: it can't predict whether any provider puts a mail in the inbox."
        ),
        word = view.word(),
    )
}

/// The sign-in code a mail holds, to copy: the one six-digit number
/// standing alone in its text, or its HTML's when it has no text (area
/// 8's codes are six digits).
fn sign_in_code(mail: &Mail) -> Option<String> {
    let text = match (&mail.text, &mail.html) {
        (Some(text), _) => text.clone(),
        (None, Some(html)) => jadpo_std::email_health::visible_text(html),
        (None, None) => return None,
    };
    let mut found: Vec<&str> = Vec::new();
    for word in text.split(|c: char| {
        c.is_whitespace() || matches!(c, ':' | ';' | '(' | ')' | '<' | '>' | '"' | '\'')
    }) {
        let word = word.trim_end_matches(['.', ',', '!', '?']);
        if word.len() == 6 && word.bytes().all(|b| b.is_ascii_digit()) && !found.contains(&word) {
            found.push(word);
        }
    }
    match found.as_slice() {
        [code] => Some((*code).to_string()),
        _ => None,
    }
}

/// SpamAssassin's part of the health view: its verdict and the rules it
/// matched, the most points first, or why it has none, never that the
/// mail is clean.
fn spam_section(spam: &Spam) -> String {
    let body = match spam {
        Spam::Ran(report) => {
            let (class, symbol) = if report.is_spam() {
                ("warn", "▲")
            } else {
                ("ok", "✓")
            };
            let rows: String = report
                .rules
                .iter()
                .map(|rule| {
                    format!(
                        "<tr class=\"{class}\"><td class=\"pts\">{points}</td><td><code>{name}</code></td><td>{description}</td></tr>",
                        class = if rule.counts() { "counts" } else { "info" },
                        points = esc(&rule.points),
                        name = esc(&rule.name),
                        description = esc(&rule.description),
                    )
                })
                .collect();
            let table = if rows.is_empty() {
                "<p class=\"muted\">No rule matched.</p>".to_string()
            } else {
                format!(
                    "<table class=\"rules\"><thead><tr><th class=\"pts\" scope=\"col\">Points</th><th scope=\"col\">Rule</th><th scope=\"col\">What it matched</th></tr></thead><tbody>{rows}</tbody></table>"
                )
            };
            format!(
                "<p class=\"verdict\"><span class=\"sym {class}\" aria-hidden=\"true\">{symbol}</span> {summary}</p>{table}{next}<p class=\"note\">Run on this machine with its network tests off, on the mail as the SMTP driver writes it.</p>",
                summary = esc(&spam.summary()),
                next = if report.is_spam() {
                    "<p>Fix the rules with the most points first: each says what it matched.</p>"
                } else {
                    ""
                },
            )
        }
        _ => {
            let (why, next) = spam.absent().unwrap_or_default();
            format!(
                "<div class=\"absent\"><p>{why}</p><p class=\"muted\">{next}.</p></div>",
                why = code_spans(&why),
                next = code_spans(&next),
            )
        }
    };
    format!(
        "<section class=\"spam\" aria-labelledby=\"spam-heading\"><h2 id=\"spam-heading\">SpamAssassin's rules</h2>{body}</section>"
    )
}

/// What a passing check says.
fn passed_title(check: Check) -> String {
    match check {
        Check::NoText => "Has a plain-text part",
        Check::ImageOnly => "Text, not only images",
        Check::BrokenHtml => "HTML parses cleanly",
        Check::LinkNotHttps => "Every link is absolute HTTPS",
        Check::LinkShortener => "No link shorteners",
        Check::CapitalSubject => "Subject not in capitals",
        Check::TooLarge => "Under Gmail's 102 KB clip",
    }
    .to_string()
}

/// `text` escaped, each `` `code` `` set as code.
fn code_spans(text: &str) -> String {
    let mut out = String::new();
    for (at, piece) in text.split('`').enumerate() {
        if at % 2 == 1 {
            out.push_str(&format!("<code>{}</code>", esc(piece)));
        } else {
            out.push_str(&esc(piece));
        }
    }
    out
}

/// A text part escaped, each web address in it a link.
fn linked(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    for link in jadpo_std::email_health::text_links(text) {
        let Some(at) = rest.find(&link) else { continue };
        out.push_str(&esc(&rest[..at]));
        out.push_str(&format!(
            "<a href=\"{href}\" target=\"_blank\" rel=\"noreferrer\">{text}</a>",
            href = esc(&link),
            text = esc(&link)
        ));
        rest = &rest[at + link.len()..];
    }
    out.push_str(&esc(rest));
    out
}

const STYLE: &str = r#"
:root{color-scheme:light dark;--bg:#ffffff;--panel:#f4f4f5;--line:#e2e3e6;--text:#1d1f23;--muted:#5d6470;--accent:#0b649f;--success:#0f7a55;--warning:#8b5400;--failure:#c3262f;--select:#e6f0f7;--code:#eceef1;
--sans:system-ui,-apple-system,"Segoe UI",Roboto,"Helvetica Neue",Arial,sans-serif;--mono:ui-monospace,SFMono-Regular,Menlo,Consolas,"Liberation Mono",monospace}
@media (prefers-color-scheme:dark){:root{--bg:#1e1e1e;--panel:#252628;--line:#34363a;--text:#e4e5e7;--muted:#969da8;--accent:#5cc3f0;--success:#4fd69c;--warning:#ebb54a;--failure:#ff7b70;--select:#1f3340;--code:#2c2e31}}
*{box-sizing:border-box}
html,body{margin:0;height:100%}
body{background:var(--bg);color:var(--text);font:14px/1.5 var(--sans);display:flex;flex-direction:column;-webkit-font-smoothing:antialiased}
::selection{background:var(--select)}
a{color:var(--accent);text-underline-offset:2px}
a:focus-visible,button:focus-visible{outline:2px solid var(--accent);outline-offset:2px;border-radius:3px}
code,pre,time,.to,.kind{font-family:var(--mono);font-size:12.5px;font-variant-numeric:tabular-nums}
code{background:var(--code);padding:1px 5px;border-radius:4px}
.bar{display:flex;align-items:baseline;gap:16px;padding:12px 20px;border-bottom:1px solid var(--line);flex:none}
.home{font-weight:650;font-size:15px;color:var(--text);text-decoration:none}
.facts{color:var(--muted)}
.fresh{font-weight:600}
.panes{display:flex;flex:1;min-height:0}
.side{width:340px;flex:none;border-right:1px solid var(--line);overflow:auto;background:var(--panel)}
.main{flex:1;min-width:0;overflow:auto;padding:24px 32px 40px}
.mails{list-style:none;margin:0;padding:0}
.mails a{display:block;padding:12px 20px;border-bottom:1px solid var(--line);color:var(--text);text-decoration:none}
.mails a:hover{background:var(--bg)}
.mails a[aria-current]{background:var(--select)}
.line{display:flex;justify-content:space-between;align-items:baseline;gap:12px;min-width:0}
.subject{font-weight:600;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.to{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--text)}
.mails time,.kind{color:var(--muted);white-space:nowrap}
.kind{display:block;font-size:12px}
.health{font-size:12px;white-space:nowrap}
.ok,.ok .sym{color:var(--success)}
.warn,.warn .sym{color:var(--warning)}
.foot{color:var(--muted);padding:12px 20px;margin:0;font-size:12.5px}
.empty{padding:32px 20px}
.empty h2{font-size:15px;margin:0 0 8px}
.empty p{margin:0 0 8px;max-width:36ch}
.muted{color:var(--muted)}
.back{display:none;margin-bottom:12px}
h1{font-size:20px;line-height:1.3;font-weight:650;margin:0 0 16px;max-width:60ch;letter-spacing:-0.01em}
.meta{display:grid;grid-template-columns:max-content 1fr;gap:4px 20px;margin:0 0 16px}
.meta dt{color:var(--muted)}
.meta dd{margin:0;font-family:var(--mono);font-size:12.5px;overflow-wrap:anywhere}
.strip{margin:0}
.strip a{margin-left:8px}
.sym.ok{color:var(--success)}
.sym.warn{color:var(--warning)}
.code{font-family:var(--mono);font-size:16px;letter-spacing:0.12em;font-weight:600;margin-right:8px}
.hidden{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
.live{margin-left:auto}
.gone{color:var(--muted)}
.caveat{margin:2px 0 20px;color:var(--muted);font-size:12.5px;max-width:72ch}
.tabs{display:flex;flex-wrap:wrap;gap:4px;border-bottom:1px solid var(--line);margin-bottom:16px}
.tabs a{padding:8px 12px;color:var(--muted);text-decoration:none;border-bottom:2px solid transparent;margin-bottom:-1px}
.tabs a:hover{color:var(--text)}
.tabs a[aria-current]{color:var(--text);border-bottom-color:var(--accent);font-weight:600}
.count{font-family:var(--mono);font-size:12px;color:var(--muted);margin-left:2px}
.count.warn{color:var(--warning)}
.note{color:var(--muted);font-size:12.5px;margin:0 0 8px}
.frame{display:block;width:100%;min-height:70vh;border:1px solid var(--line);border-radius:6px;background:#fff}
.text{white-space:pre-wrap;overflow-wrap:anywhere;margin:0;padding:16px;background:var(--panel);border-radius:6px;max-width:80ch;line-height:1.6}
.absent{padding:16px;background:var(--panel);border-radius:6px;max-width:72ch}
.absent p{margin:0 0 6px}
.links{list-style:none;margin:0;padding:0;max-width:90ch}
.links li{display:flex;gap:12px;padding:12px 0;border-bottom:1px solid var(--line)}
.links .n{font-family:var(--mono);color:var(--muted);min-width:2ch;text-align:right}
.link{min-width:0;flex:1}
.link>a,.link>span{font-family:var(--mono);font-size:12.5px;overflow-wrap:anywhere}
.actions{display:flex;flex-wrap:wrap;align-items:center;gap:12px;margin-top:6px}
.actions code{color:var(--muted)}
button{font:inherit;font-size:12.5px;color:var(--text);background:var(--bg);border:1px solid var(--line);border-radius:6px;padding:3px 10px;cursor:pointer}
button:hover{border-color:var(--muted)}
.checks{list-style:none;margin:0;padding:0;max-width:72ch}
.checks li{display:flex;gap:10px;padding:10px 0;border-bottom:1px solid var(--line)}
.checks li>div{min-width:0}
.checks h2{font-size:14px;margin:0;color:var(--text)}
.checks li.ok h2{font-weight:400;color:var(--muted)}
.checks code{overflow-wrap:anywhere}
.checks p{margin:4px 0 0;color:var(--text)}
.checks .found{color:var(--muted)}
.spam{margin:28px 0 0;max-width:72ch}
.spam h2{font-size:14px;font-weight:650;margin:0 0 10px}
.spam-note{color:var(--muted)}
.verdict{margin:0 0 12px}
.rules{border-collapse:collapse;width:100%;margin:0 0 10px}
.rules th{text-align:left;font-weight:400;color:var(--muted);font-size:12.5px;padding:0 12px 6px 0;border-bottom:1px solid var(--line)}
.rules td{padding:8px 12px 8px 0;border-bottom:1px solid var(--line);vertical-align:baseline}
.rules code{background:none;padding:0;overflow-wrap:anywhere}
.rules .pts{text-align:right;font-family:var(--mono);font-size:12.5px;font-variant-numeric:tabular-nums;white-space:nowrap;width:5ch}
.rules .counts .pts{color:var(--warning);font-weight:600}
.rules .info .pts,.rules .info td:last-child{color:var(--muted)}
.disclaimer{margin:16px 0 0;color:var(--muted);max-width:72ch}
.missing{max-width:60ch}
@media (max-width:820px){
.side{width:100%;border-right:0}
.has-mail .side{display:none}
.is-index .main,.is-empty .main{display:none}
.has-mail .back{display:inline-block}
.main{padding:16px}
.facts{display:none}
}
"#;

const SCRIPT: &str = r#"
(function(){
var newest=document.body.dataset.newest||"";
var atNewest=location.pathname==="/"&&!location.search;
function pad(n){return String(n).padStart(2,"0")}
document.querySelectorAll("time[datetime]").forEach(function(t){
var d=new Date(t.getAttribute("datetime"));if(isNaN(d))return;
var clock=pad(d.getHours())+":"+pad(d.getMinutes())+":"+pad(d.getSeconds());
var day=d.getFullYear()+"-"+pad(d.getMonth()+1)+"-"+pad(d.getDate());
var today=new Date().toDateString()===d.toDateString();
t.textContent=t.hasAttribute("data-short")?(today?clock:day):(day+" "+clock);
t.title=d.toString();
});
document.addEventListener("click",function(e){
var b=e.target.closest("button[data-copy]");if(!b)return;
navigator.clipboard.writeText(b.getAttribute("data-copy")).then(function(){
var was=b.textContent;b.textContent="Copied";setTimeout(function(){b.textContent=was},1500);
},function(){b.textContent="Copy failed"});
});
setInterval(function(){
fetch("/inbox.json",{cache:"no-store"}).then(function(r){return r.json()}).then(function(j){
document.getElementById("gone").hidden=true;
if((j.newest||"")===newest)return;
var main=document.querySelector(".main");
if(atNewest&&window.scrollY===0&&(!main||main.scrollTop===0)){location.reload()}else{document.getElementById("fresh").hidden=false}
}).catch(function(){document.getElementById("gone").hidden=false});
},2000);
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn get(root: &Path, path: &str, query: Option<&str>) -> Response {
        answer(
            root,
            &Request {
                method: "GET".into(),
                path: path.into(),
                query: query.map(str::to_string),
                headers: vec![("host".into(), "localhost:3001".into())],
                ..Request::default()
            },
        )
    }

    fn body(response: &Response) -> String {
        String::from_utf8_lossy(&response.body).into_owned()
    }

    #[test]
    fn a_six_digit_code_standing_alone_is_offered_to_copy() {
        let mail = |text: &str| Mail {
            id: "mail_1".into(),
            number: 1,
            at: String::new(),
            email: String::new(),
            trace: None,
            from: String::new(),
            to: String::new(),
            subject: String::new(),
            html: None,
            text: Some(text.to_string()),
            attachments: Vec::new(),
            spam: Spam::NotRun,
        };
        assert_eq!(
            sign_in_code(&mail("Or enter this code: 482913.\n")),
            Some("482913".to_string())
        );
        assert_eq!(
            sign_in_code(&mail("Paid £48.00 on 2026-10-20, order 1234567")),
            None
        );
        assert_eq!(sign_in_code(&mail("Codes 111111 and 222222")), None);
    }

    #[test]
    fn only_requests_to_this_machine_by_name_are_answered() {
        assert!(local_host(Some("localhost:3001")));
        assert!(local_host(Some("127.0.0.1:3001")));
        assert!(local_host(Some("[::1]:3001")));
        assert!(local_host(Some("inbox.localhost")));
        assert!(!local_host(Some("evil.example:3001")));
        assert!(!local_host(None));
        let root = std::env::temp_dir().join(format!("jadpo-inbox-page-{}", std::process::id()));
        let response = answer(
            &root,
            &Request {
                method: "GET".into(),
                path: "/".into(),
                headers: vec![("host".into(), "evil.example".into())],
                ..Request::default()
            },
        );
        assert_eq!(response.status, 421);
    }

    #[test]
    fn the_page_shows_the_newest_mail_its_parts_and_its_health() {
        let root =
            std::env::temp_dir().join(format!("jadpo-inbox-page-mail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let empty = get(&root, "/", None);
        assert_eq!(empty.status, 200);
        assert!(body(&empty).contains("No mail yet"));
        let message = jadpo_value::Value::record(
            jadpo_std::email::MESSAGE,
            jadpo_std::email::MESSAGE_FIELDS
                .iter()
                .zip([
                    jadpo_value::Value::text("hello@notes.example"),
                    jadpo_value::Value::text("ada@notes.example"),
                    jadpo_value::Value::text("Sign in <now>"),
                    jadpo_value::Value::text(
                        "<html><head></head><body><a href=\"http://localhost:5173/sign-in?token=abc\">Sign in</a> 123456</body></html>",
                    ),
                    jadpo_value::Value::None,
                    jadpo_value::Value::List(Vec::new().into()),
                ])
                .map(|(name, value)| (std::rc::Rc::from(*name), value))
                .collect(),
        );
        dev_inbox::keep(&root, &message, None, 0, Spam::NotInstalled).expect("kept");
        let page = body(&get(&root, "/", None));
        assert!(page.contains("Sign in &lt;now&gt;"), "{page}");
        assert!(
            page.contains("Health: <strong>1 problem</strong> · 6 of 7 checks pass"),
            "{page}"
        );
        assert!(page.contains("can&#39;t predict"), "{page}");
        // SpamAssassin isn't installed: said, never that the mail is clean.
        assert!(page.contains("· SpamAssassin not run"), "{page}");
        let health = body(&get(&root, "/mail/mail_1", Some("view=health")));
        assert!(
            health.contains("That doesn&#39;t make the mail clean."),
            "{health}"
        );
        let links = body(&get(&root, "/mail/mail_1", Some("view=links")));
        assert!(links.contains("jadpo email open mail_1 1"));
        let text = body(&get(&root, "/mail/mail_1", Some("view=text")));
        assert!(text.contains("no plain-text part"));
        let framed = get(&root, "/mail/mail_1/body", None);
        assert!(body(&framed).contains("<head><base target=\"_blank\"></head>"));
        assert!(
            framed
                .headers
                .iter()
                .any(|(name, value)| name == "content-security-policy"
                    && value.starts_with("sandbox"))
        );
        assert_eq!(get(&root, "/mail/mail_9", None).status, 404);
        assert_eq!(get(&root, "/nope", None).status, 404);
        let json = body(&get(&root, "/inbox.json", None));
        assert_eq!(json, "{\"count\":1,\"newest\":\"mail_1\"}\n");
        // A mail SpamAssassin calls spam: one more problem, its rules shown.
        let report = crate::spam::Report {
            score: "6.2".to_string(),
            required: "5.0".to_string(),
            rules: vec![
                crate::spam::Rule {
                    name: "HTML_IMAGE_ONLY_08".to_string(),
                    points: "1.5".to_string(),
                    description: "BODY: HTML: images with 400-800 bytes of words".to_string(),
                },
                crate::spam::Rule {
                    name: "HTML_MESSAGE".to_string(),
                    points: "0.0".to_string(),
                    description: "BODY: HTML included in message".to_string(),
                },
            ],
        };
        dev_inbox::keep(&root, &message, None, 1, Spam::Ran(report)).expect("kept");
        let page = body(&get(&root, "/mail/mail_2", Some("view=health")));
        assert!(
            page.contains("Health: <strong>2 problems</strong> · 6 of 7 checks pass"),
            "{page}"
        );
        assert!(
            page.contains("SpamAssassin calls it spam: 6.2 points, spam from 5.0"),
            "{page}"
        );
        assert!(
            page.contains("<tr class=\"counts\"><td class=\"pts\">1.5</td><td><code>HTML_IMAGE_ONLY_08</code></td>"),
            "{page}"
        );
        assert!(page.contains("<tr class=\"info\">"), "{page}");
    }
}
