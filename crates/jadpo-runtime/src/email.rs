//! Sending an email (area 16, "Email"; chunk 7.3): a delivery whose target
//! is [`jadpo_std::subscriber::Target::Email`] renders the email it was
//! handed whole, then sends the message through the connection `email:`
//! names, on the capability seam.
//!
//! [`message`] renders it: each of its templates' callables run by the
//! executor with its fields, each wrapped in the shared layout's with the
//! subject and the locale; its subject, its member in its domain's
//! language files, written in its locale; the address it is from; and
//! each `File` field read from its store as an attachment. The driver is
//! sent a record of [`jadpo_std::email::MESSAGE_FIELDS`].
//!
//! Jadpo's own magic-link email (chunk 7.3b) is written here instead of by
//! templates: its link and code made as it is sent
//! ([`crate::http::magic_links::mint`]), its lines in Jadpo's wording or
//! the application's (`jadpo: { }`), its body wrapped in the application's
//! layout when it has one, or in a plain document of Jadpo's. With
//! `magic_link: { email: }` (chunk 7.3b2) the application's own email is
//! filled from the link and the code instead and rendered as any of its
//! emails is.

use std::rc::Rc;

use jadpo_std::email::{
    ATTACHMENT, ATTACHMENT_FIELDS, BuiltinEmail, EmailPlan, EmailsPlan, MESSAGE, MESSAGE_FIELDS,
    MagicLinkEmailPlan, Sender, escape_html,
};
use jadpo_value::Value;

use crate::files::{Done, read_whole};
use crate::{Ctx, Executor, FaultKind, Frame, Outcome};

/// The message `value`, an email emitted as `email`, is sent as: or the
/// outcome its delivery is settled with when it can't be rendered.
pub async fn message(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    emails: &EmailsPlan,
    email: &EmailPlan,
    value: &Value,
) -> Result<Value, Outcome> {
    if let Some(BuiltinEmail::MagicLink(plan)) = &email.builtin {
        return magic_link(executor, ctx, emails, plan, value).await;
    }
    rendered(executor, ctx, emails, email, value).await
}

/// The message an email of the application's is sent as, rendered from
/// its templates, its subject and its attachments.
async fn rendered(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    emails: &EmailsPlan,
    email: &EmailPlan,
    value: &Value,
) -> Result<Value, Outcome> {
    let Value::Record(record) = value else {
        return Err(defect(ctx, "an email that isn't a record"));
    };
    let field = |name: &str| record.get(name).cloned().unwrap_or(Value::None);
    let args: Vec<Value> = email.fields.iter().map(|name| field(name)).collect();
    let locale = field("locale");
    let to = field("to");
    // Its subject, written in its locale.
    let subject = match (&email.subject, &emails.locales) {
        (Some(entry), Some(locales)) => {
            let given: Vec<Value> = entry
                .params
                .iter()
                .map(|param| field(&param.name))
                .collect();
            match jadpo_value::present::translate(entry, &locale, &given, locales) {
                Ok(text) => text,
                Err(fault) => return Err(Outcome::Fault(ctx.fault(fault.into(), Frame::ENTRY))),
            }
        }
        _ => return Err(defect(ctx, "an email with no subject")),
    };
    let from = sender(executor, ctx, emails)?;
    let html = render(
        executor,
        ctx,
        email.html,
        emails.layout_html,
        &args,
        &subject,
        &locale,
    )
    .await?;
    let text = render(
        executor,
        ctx,
        email.text,
        emails.layout_text,
        &args,
        &subject,
        &locale,
    )
    .await?;
    // Each attachment, read whole from its store.
    let mut attachments = Vec::new();
    for name in &email.attachments {
        let file = field(name);
        if file == Value::None {
            continue;
        }
        let Some(stored) = crate::files::stored(&file) else {
            return Err(defect(ctx, "an attachment that isn't a stored file"));
        };
        let content = match read_whole(ctx, &file).await {
            Done::Value(bytes) => bytes,
            Done::Failed(failure) => return Err(Outcome::Failed(failure)),
            Done::Fault(kind) => return Err(Outcome::Fault(ctx.fault(kind, Frame::ENTRY))),
            Done::Broke(_) => return Err(defect(ctx, "an attachment's store refused a read")),
        };
        let Value::Record(file) = &file else {
            continue;
        };
        let content_type = file
            .get("type")
            .and_then(crate::files::media_type)
            .unwrap_or("application/octet-stream");
        let file_name = match file.get("name") {
            Some(Value::Text(name)) => name.to_string(),
            _ => stored
                .key
                .rsplit('/')
                .next()
                .unwrap_or(&stored.key)
                .to_string(),
        };
        attachments.push(Value::record(
            ATTACHMENT,
            ATTACHMENT_FIELDS
                .iter()
                .zip([Value::text(&file_name), Value::text(content_type), content])
                .map(|(name, value)| (Rc::from(*name), value))
                .collect(),
        ));
    }
    Ok(Value::record(
        MESSAGE,
        MESSAGE_FIELDS
            .iter()
            .zip([
                from,
                to,
                subject,
                html,
                text,
                Value::List(attachments.into()),
            ])
            .map(|(name, value)| (Rc::from(*name), value))
            .collect(),
    ))
}

/// Who every email is from: `email: { from: }`.
fn sender(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    emails: &EmailsPlan,
) -> Result<Value, Outcome> {
    match &emails.from {
        Some(Sender::Address(address)) => Ok(Value::text(address)),
        Some(Sender::Setting(at)) => {
            match ctx.setting(*at).cloned().or_else(|| executor.setting(*at)) {
                Some(value) => Ok(value),
                None => Err(defect(ctx, "an email whose `from:` setting has no value")),
            }
        }
        None => Err(defect(ctx, "an email with no `from:`")),
    }
}

/// Jadpo's own magic-link email (area 16, "Built-in emails", levels 1 to
/// 3), as it is sent: its link and code made now, its lines worded in its
/// locale, its HTML and text wrapped in the application's layout or a
/// plain document of Jadpo's. A request used or replaced since sends
/// nothing: the delivery succeeds with no message.
async fn magic_link(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    emails: &EmailsPlan,
    plan: &MagicLinkEmailPlan,
    value: &Value,
) -> Result<Value, Outcome> {
    let minted = match crate::http::magic_links::mint(executor, ctx, plan, value).await {
        Ok(Some(minted)) => minted,
        Ok(None) => return Err(Outcome::Value(Value::Unit)),
        Err("unavailable") => return Err(Outcome::Failed(Box::new(crate::unavailable()))),
        Err(what) => return Err(defect(ctx, what)),
    };
    let locale = match value {
        Value::Record(record) => record.get("locale").cloned().unwrap_or(Value::None),
        _ => Value::None,
    };
    let Value::Variant(chosen) = &locale else {
        return Err(defect(ctx, "a magic-link email with no locale"));
    };
    let ordinal = chosen.ordinal;
    let tag = jadpo_std::LOCALES
        .get(ordinal as usize)
        .map_or("en-GB", |(_, tag)| *tag);
    let expires_in = match jadpo_value::present::duration_words(plan.expires_after_ms, tag) {
        Ok(words) => words,
        Err(fault) => return Err(Outcome::Fault(ctx.fault(fault.into(), Frame::ENTRY))),
    };
    // The application's own email (level 4): its fields picked from
    // `MagicLinkEmail`, filled now, and rendered as any of its emails is.
    if let Some(key) = &plan.own {
        let Some(own) = emails.email(key) else {
            return Err(defect(ctx, "a sign-in email this build doesn't have"));
        };
        let filled: Vec<(Rc<str>, Value)> = own
            .fields
            .iter()
            .map(|name| {
                let value = match name.as_str() {
                    "to" | "address" => Value::text(&minted.address),
                    "locale" => locale.clone(),
                    "sign_in_link" => Value::text(&minted.link),
                    "code" => Value::text(&minted.code),
                    "expires_in" => Value::text(&expires_in),
                    _ => Value::None,
                };
                (Rc::from(name.as_str()), value)
            })
            .collect();
        let value = Value::record(&own.name, filled);
        return rendered(executor, ctx, emails, own, &value).await;
    }
    // Each line in the email's locale, given its fields and the
    // application's name.
    let mut line = |key: &str, given: &[(&str, &str)]| -> Result<String, Outcome> {
        let Some(entry) = plan.message(key) else {
            return Err(defect(
                ctx,
                "a magic-link email missing one of Jadpo's messages",
            ));
        };
        let args: Vec<Value> = entry
            .params
            .iter()
            .map(|param| {
                if param.name == jadpo_std::jadpo_text::APP_NAME {
                    return Value::text(&plan.app_name);
                }
                given
                    .iter()
                    .find(|(name, _)| *name == param.name)
                    .map_or(Value::None, |(_, text)| Value::text(text))
            })
            .collect();
        match jadpo_value::present::translate(entry, &locale, &args, &plan.locales) {
            Ok(Value::Text(text)) => Ok(text.to_string()),
            Ok(_) => Err(defect(ctx, "a magic-link line that isn't text")),
            Err(fault) => Err(Outcome::Fault(ctx.fault(fault.into(), Frame::ENTRY))),
        }
    };
    let subject = line("magic_link_email", &[])?;
    let heading = line("magic_link_heading", &[])?;
    let intro = line("magic_link_intro", &[("address", &minted.address)])?;
    let button = line("magic_link_button", &[])?;
    let code_line = line("magic_link_code_line", &[("code", &minted.code)])?;
    let expiry = line("magic_link_expiry_line", &[("expires_in", &expires_in)])?;
    let ignore = line("magic_link_ignore_line", &[])?;
    let rtl = matches!(
        jadpo_value::locale::direction(ordinal),
        Value::Variant(direction) if &*direction.name == jadpo_std::TextDirection::RightToLeft.name()
    );
    let direction = if rtl { "rtl" } else { "ltr" };
    // The code stands out in its line, set for reading aloud and copying.
    let code_html = escape_html(&code_line).replacen(
        &minted.code,
        &format!(
            "<strong style=\"font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:20px;letter-spacing:0.12em;font-variant-numeric:tabular-nums;white-space:nowrap;\">{}</strong>",
            minted.code
        ),
        1,
    );
    let html = format!(
        "<div dir=\"{direction}\" style=\"max-width:480px;font-size:16px;line-height:1.5;\">\
<h1 style=\"margin:0 0 16px;font-size:24px;line-height:1.25;font-weight:600;\">{heading}</h1>\
<p style=\"margin:0 0 24px;\">{intro}</p>\
<p style=\"margin:0 0 24px;\"><a href=\"{link}\" style=\"display:inline-block;padding:12px 24px;border-radius:6px;background-color:#1f2937;color:#ffffff;font-weight:600;text-decoration:none;\">{button}</a></p>\
<p style=\"margin:0 0 24px;\">{code_html}</p>\
<p style=\"margin:0 0 8px;font-size:14px;\">{expiry}</p>\
<p style=\"margin:0;font-size:14px;\">{ignore}</p>\
</div>",
        heading = escape_html(&heading),
        intro = escape_html(&intro),
        link = escape_html(&minted.link),
        button = escape_html(&button),
        expiry = escape_html(&expiry),
        ignore = escape_html(&ignore),
    );
    let text = format!(
        "{heading}\n\n{intro}\n\n{button}: {link}\n\n{code_line}\n\n{expiry}\n\n{ignore}\n",
        link = minted.link,
    );
    let subject_value = Value::text(&subject);
    let (html, text) = match (emails.layout_html, emails.layout_text) {
        (None, None) => (
            Value::text(&format!(
                "<!DOCTYPE html>\n<html lang=\"{tag}\" dir=\"{direction}\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{title}</title></head><body style=\"margin:0;padding:24px;\">{html}</body></html>\n",
                title = escape_html(&subject),
            )),
            Value::text(&text),
        ),
        (layout_html, layout_text) => (
            wrap(executor, ctx, layout_html, html, &subject_value, &locale).await?,
            wrap(executor, ctx, layout_text, text, &subject_value, &locale).await?,
        ),
    };
    let from = sender(executor, ctx, emails)?;
    Ok(Value::record(
        MESSAGE,
        MESSAGE_FIELDS
            .iter()
            .zip([
                from,
                Value::text(&minted.address),
                subject_value,
                html,
                text,
                Value::List(Vec::new().into()),
            ])
            .map(|(name, value)| (Rc::from(*name), value))
            .collect(),
    ))
}

/// `body` wrapped in the layout's callable, when it has one.
async fn wrap(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    layout: Option<u32>,
    body: String,
    subject: &Value,
    locale: &Value,
) -> Result<Value, Outcome> {
    let Some(layout) = layout else {
        return Ok(Value::text(&body));
    };
    match executor
        .run(
            ctx,
            layout,
            vec![Value::text(&body), subject.clone(), locale.clone()],
        )
        .await
    {
        Outcome::Value(text @ Value::Text(_)) => Ok(text),
        Outcome::Value(_) => Err(defect(ctx, "a layout that gave no text")),
        other => Err(other),
    }
}

/// One template's text, wrapped in its layout: `none` without the
/// template.
async fn render(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    template: Option<u32>,
    layout: Option<u32>,
    args: &[Value],
    subject: &Value,
    locale: &Value,
) -> Result<Value, Outcome> {
    let Some(template) = template else {
        return Ok(Value::None);
    };
    let body = match executor.run(ctx, template, args.to_vec()).await {
        Outcome::Value(text @ Value::Text(_)) => text,
        Outcome::Value(_) => return Err(defect(ctx, "a template that gave no text")),
        other => return Err(other),
    };
    let Some(layout) = layout else {
        return Ok(body);
    };
    match executor
        .run(ctx, layout, vec![body, subject.clone(), locale.clone()])
        .await
    {
        Outcome::Value(text @ Value::Text(_)) => Ok(text),
        Outcome::Value(_) => Err(defect(ctx, "a layout that gave no text")),
        other => Err(other),
    }
}

/// A message a test's email connection caught, as `email.sent` lists it
/// (chunk 7.3c): a `SentEmail` of its address, sender, subject, HTML and
/// text, every link in it, and its attachments' file names.
pub fn sent_email(message: &Value) -> Value {
    let field = |name: &str| match message {
        Value::Record(record) => record.get(name).cloned().unwrap_or(Value::None),
        _ => Value::None,
    };
    let text_of = |value: &Value| match value {
        Value::Text(text) => Some(text.to_string()),
        _ => None,
    };
    let html = field("html");
    let text = field("text");
    let mut links: Vec<String> = Vec::new();
    for found in text_of(&html)
        .map(|html| html_links(&html))
        .unwrap_or_default()
        .into_iter()
        .chain(
            text_of(&text)
                .map(|text| text_links(&text))
                .unwrap_or_default(),
        )
    {
        if !links.contains(&found) {
            links.push(found);
        }
    }
    let attachments: Vec<Value> = match field("attachments") {
        Value::List(items) => items
            .iter()
            .filter_map(|item| match item {
                Value::Record(record) => record.get("name").cloned(),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    Value::record(
        jadpo_std::email::SENT_EMAIL,
        jadpo_std::email::SENT_EMAIL_FIELDS
            .iter()
            .zip([
                field("to"),
                field("from"),
                field("subject"),
                html,
                text,
                Value::List(
                    links
                        .iter()
                        .map(|link| Value::text(link))
                        .collect::<Vec<_>>()
                        .into(),
                ),
                Value::List(attachments.into()),
            ])
            .map(|(name, value)| (Rc::from(*name), value))
            .collect(),
    )
}

/// Every `href` in `html`, its character references read back, in order.
pub fn html_links(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find("href=") {
        let start = from + at + "href=".len();
        from = start;
        let rest = &html[start..];
        let (value, used) = match rest.chars().next() {
            Some(quote @ ('"' | '\'')) => match rest[1..].find(quote) {
                Some(end) => (&rest[1..=end], end + 2),
                None => continue,
            },
            Some(_) => {
                let end = rest
                    .find(|c: char| c.is_whitespace() || c == '>')
                    .unwrap_or(rest.len());
                (&rest[..end], end)
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
        .filter_map(|scheme| text[from..].find(scheme))
        .min()
    {
        let start = from + at;
        let end = text[start..]
            .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"'))
            .map_or(text.len(), |end| start + end);
        let link = text[start..end].trim_end_matches(['.', ',', ')', ';', ':', '!', '?']);
        if link.len() > "https://".len() {
            out.push(link.to_string());
        }
        from = end.max(start + 1);
    }
    out
}

/// `&amp;`, `&lt;`, `&gt;`, `&quot;` and `&#39;`, as [`escape_html`]
/// writes them, read back.
fn unescape_html(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn defect(ctx: &mut Ctx<'_>, what: &str) -> Outcome {
    Outcome::Fault(ctx.fault(FaultKind::Defect(what.to_string()), Frame::ENTRY))
}
