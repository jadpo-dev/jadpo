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

use std::rc::Rc;

use jadpo_std::email::{
    ATTACHMENT, ATTACHMENT_FIELDS, EmailPlan, EmailsPlan, MESSAGE, MESSAGE_FIELDS, Sender,
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
    let from = match &emails.from {
        Some(Sender::Address(address)) => Value::text(address),
        Some(Sender::Setting(at)) => {
            match ctx.setting(*at).cloned().or_else(|| executor.setting(*at)) {
                Some(value) => value,
                None => return Err(defect(ctx, "an email whose `from:` setting has no value")),
            }
        }
        None => return Err(defect(ctx, "an email with no `from:`")),
    };
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

fn defect(ctx: &mut Ctx<'_>, what: &str) -> Outcome {
    Outcome::Fault(ctx.fault(FaultKind::Defect(what.to_string()), Frame::ENTRY))
}
