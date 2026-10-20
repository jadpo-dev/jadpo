//! `email.health_problem` (area 16, "A mail health score"; chunk 7.3c2):
//! the mail health score's checks, run on each email's and partial's
//! templates as written, wherever what a check reads is known before the
//! mail is sent: a missing `.txt`, a link written out that isn't absolute
//! HTTPS or goes through a shortener, HTML that doesn't parse cleanly,
//! images with no text beside them, a subject worded in capitals, and a
//! template already over Gmail's clip. What a template writes from its
//! fields is read only in the dev inbox, once it is rendered.
//!
//! Each finding is declinable, on the email's or partial's type: the
//! score is an early indication, and a mail may break a rule on purpose.

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_std::email_health::{self, Check};

use crate::emails::analysis;

/// Every check a template would fail.
pub fn health_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let analysis = analysis(db, project);
    let records = project.records(db);
    let text_of = |path: &Option<String>| -> Option<(String, String)> {
        let path = path.as_ref()?;
        let record = records.iter().find(|record| &record.path == path)?;
        Some((path.clone(), record.text.clone().unwrap_or_default()))
    };
    let mut out = Vec::new();
    let plan = crate::translations::plan(db, project);
    for email in &analysis.emails {
        let html = text_of(&email.html);
        let text = text_of(&email.text);
        let at = &email.name_span;
        let name = &email.type_name;
        if let (Some((path, _)), None) = (&html, &text) {
            let stem = path.strip_suffix(".html").unwrap_or(path);
            out.push(finding(
                format!("`{name}` has no plain-text part"),
                at,
                "HTML only",
                format!(
                    "Filters score mail with HTML and no plain text as spam, and some readers show only text; `{}` has no `{}.txt` beside it.",
                    file(path),
                    file(stem)
                ),
                format!("Add `{stem}.txt` with the same words as plain text"),
            ));
        }
        template_checks(&mut out, name, at, html.as_ref(), text.as_ref(), true);
        // Its subject, as each locale words it.
        if let Some(scope) = email.scope()
            && let Some((_, entry)) = plan.entry(&format!("{scope}.{}", email.subject))
        {
            for (locale, wording) in &entry.wordings {
                let templates: Vec<&jadpo_std::translation::Template> = match wording {
                    jadpo_std::translation::Wording::Plain(template) => vec![template],
                    jadpo_std::translation::Wording::Variants(variants) => {
                        variants.iter().map(|(_, template)| template).collect()
                    }
                    jadpo_std::translation::Wording::Forms(forms) => {
                        forms.iter().map(|(_, template)| template).collect()
                    }
                };
                let Some(written) = templates.iter().find_map(|template| {
                    let literal: String = template
                        .pieces
                        .iter()
                        .filter_map(|piece| match piece {
                            jadpo_std::translation::Piece::Text(text) => Some(text.as_str()),
                            _ => None,
                        })
                        .collect();
                    email_health::capitals(&literal).then(|| template.written())
                }) else {
                    continue;
                };
                let locale = jadpo_std::LOCALES
                    .get(*locale as usize)
                    .map_or("its locale", |(name, _)| *name);
                out.push(finding(
                    format!("`{name}`'s subject is in capitals"),
                    at,
                    "subject in capitals",
                    format!(
                        "Filters score a subject written in capitals as shouting; its `{locale}` wording, \u{201c}{written}\u{201d}, has no lower-case letters."
                    ),
                    format!(
                        "Word `{}` in sentence case in `{scope}`'s `translations/{locale}.jadpo`",
                        email.subject
                    ),
                ));
                break;
            }
        }
    }
    for partial in &analysis.partials {
        template_checks(
            &mut out,
            &partial.type_name,
            &partial.name_span,
            text_of(&partial.html).as_ref(),
            text_of(&partial.text).as_ref(),
            false,
        );
    }
    out
}

/// The checks a template's own text answers: its links, its HTML's
/// structure, images with no text and its size. `whole` is an email's
/// own template, which holds the mail's words; a partial is a piece of
/// one, so only its links and structure are checked.
fn template_checks(
    out: &mut Vec<Diagnostic>,
    name: &str,
    at: &Span,
    html: Option<&(String, String)>,
    text: Option<&(String, String)>,
    whole: bool,
) {
    // Links written out whole; one a template builds is read once sent.
    let mut links: Vec<(String, String)> = Vec::new();
    if let Some((path, html)) = html {
        links.extend(
            email_health::html_links(html)
                .into_iter()
                .map(|link| (link, path.clone())),
        );
    }
    if let Some((path, text)) = text {
        links.extend(
            email_health::text_links(text)
                .into_iter()
                .map(|link| (link, path.clone())),
        );
    }
    links.retain(|(link, _)| !link.contains("{{"));
    for check in [Check::LinkNotHttps, Check::LinkShortener] {
        let Some((link, path)) = links
            .iter()
            .find(|(link, _)| email_health::link_problem(link, false) == Some(check))
        else {
            continue;
        };
        let (headline, reason, next) = match check {
            Check::LinkShortener => (
                format!("`{name}` links through a shortener"),
                format!(
                    "`{}` links to `{link}`; shared shorteners hide where a link goes, and their domains are on block lists.",
                    file(path)
                ),
                "Link to your own domain directly".to_string(),
            ),
            _ => (
                format!("`{name}` has a link that isn't absolute HTTPS"),
                format!(
                    "`{}` links to `{link}`; {}.",
                    file(path),
                    if link.to_ascii_lowercase().starts_with("http://") {
                        "filters distrust plain `http://`, and the reader's click goes unencrypted"
                    } else {
                        "a mail has no page to resolve a relative link against, so it leads nowhere"
                    }
                ),
                "Link to a full `https://` address".to_string(),
            ),
        };
        out.push(finding(
            headline,
            at,
            &format!("link in {}", file(path)),
            reason,
            next,
        ));
    }
    let Some((path, html)) = html else {
        return;
    };
    // What the template writes, its `{{ }}` left out. A condition can
    // open an element in one branch and close it in another, so HTML with
    // one is read only once rendered.
    let branches = html.contains("{{ if") || html.contains("{{if") || html.contains("{{ else");
    let literal = without_expressions(html);
    if !branches && let Some(problem) = email_health::html_problem(&literal) {
        out.push(finding(
            format!("`{}` doesn't parse cleanly", file(path)),
            at,
            &format!("broken HTML in {}", file(path)),
            format!(
                "{problem} Mail clients repair broken HTML each their own way, and filters score it."
            ),
            "Close each element, in the reverse order it was opened".to_string(),
        ));
    }
    // Images with no text written beside them.
    if whole
        && email_health::images(&literal) > 0
        && !email_health::visible_text(html).contains("{{")
        && let Some(detail) = email_health::image_only(&literal)
    {
        out.push(finding(
            format!("`{}` is images with almost no text", file(path)),
            at,
            &format!("images only in {}", file(path)),
            format!(
                "{detail} Filters can't read images, and many clients block them until the reader asks."
            ),
            "Write the message as text in the HTML, keeping images to support it".to_string(),
        ));
    }
    if html.len() > email_health::CLIPPED_AT {
        out.push(finding(
            format!("`{}` is over 102 KB", file(path)),
            at,
            &format!("{} KB of HTML", html.len().div_ceil(1024)),
            "Gmail clips HTML past 102 KB, hiding the rest behind a link, often the footer and any unsubscribe link.".to_string(),
            "Shorten the template, or move what it repeats into a partial that writes less".to_string(),
        ));
    }
}

fn finding(headline: String, at: &Span, label: &str, reason: String, next: String) -> Diagnostic {
    Diagnostic::new(&rule::EMAIL_HEALTH_PROBLEM, headline)
        .at(at.clone(), label)
        .reason(reason)
        .next(next)
}

/// A path's last part: `welcome_email.html`.
fn file(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// `template` with each `{{ ... }}` left out.
fn without_expressions(template: &str) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        match rest[start..].find("}}") {
            Some(end) => rest = &rest[start + end + 2..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expressions_are_left_out() {
        assert_eq!(
            without_expressions("<a href=\"{{ url }}\">{{ label }}</a>{{ x"),
            "<a href=\"\"></a>"
        );
    }
}
