//! The local issuer's account-chooser page (area 8, "Providers", "In
//! `jadpo dev`"; area 14, `jadpo dev`; chunk A7c): where `jadpo dev`
//! sends the browser in a sign-in provider's place, so a developer signs
//! in as a test account with none of the provider's real settings.
//!
//! - **`GET /<provider>/authorize?…`** is the provider's sign-in address
//!   as the start route made it. The page lists the provider's test
//!   accounts, a form adding one (subject, email, whether the provider
//!   verified it, name) and a cancel button.
//! - **`POST /<provider>/authorize`** answers the choice as the provider
//!   would: `303` to the app's callback with a code the local issuer gave
//!   for the account, or with `error=access_denied` for a cancel.
//!
//! The request is checked as a provider checks its registered client: the
//! provider must be one of the app's, and the client ID and the return
//! address the app's own, or the page says why and sends the browser
//! nowhere. Only a request addressed to this machine by name is answered,
//! and a choice only from the page itself, so another site can't sign a
//! developer in. The page is built only with the `testing` feature, so a
//! deployable holds none of it (`tools/checks/deployable.py`).

use jadpo_std::email::escape_html as esc;
use jadpo_std::http::IdentityPlan;

use super::local_issuer::{Account, LocalIssuer};
use super::{Request, Response, percent_encode, query_values};

/// A sign-in provider as the app registered it: its client ID and the
/// address the provider sends the browser back to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registration {
    pub provider: String,
    pub client_id: String,
    pub redirect_uri: String,
}

/// The app's sign-in providers whose settings hold, as it started.
pub fn registrations(
    started: &crate::Started,
    identity: Option<&IdentityPlan>,
) -> Vec<Registration> {
    let Some(providers) = identity.and_then(|identity| identity.providers.as_ref()) else {
        return Vec::new();
    };
    let Some(origin) = started.auth().origin() else {
        return Vec::new();
    };
    providers
        .providers
        .iter()
        .enumerate()
        .filter_map(|(at, provider)| {
            let settings = started.auth().provider(u32::try_from(at).ok()?)?;
            Some(Registration {
                provider: provider.name.clone(),
                client_id: settings.keys.audience().to_string(),
                redirect_uri: format!("{origin}/auth/{}/callback", provider.name),
            })
        })
        .collect()
}

/// Where the test accounts are kept, by provider.
pub trait Accounts {
    /// The provider's test accounts, in the order they were added.
    fn list(&self, provider: &str) -> Vec<Account>;
    /// Keeps `account`, in place of one with its subject.
    fn keep(&self, provider: &str, account: &Account);
}

/// What answering the page reads.
pub struct Chooser<'a> {
    pub issuer: &'a LocalIssuer,
    pub registrations: &'a [Registration],
    pub accounts: &'a dyn Accounts,
    /// Bytes for a new account's subject.
    pub random: &'a dyn Fn(&mut [u8]) -> bool,
    /// Now, in milliseconds since 1970.
    pub now: i64,
}

impl std::fmt::Debug for Chooser<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Chooser")
            .field("registrations", &self.registrations)
            .finish_non_exhaustive()
    }
}

/// What the app's start route asked the provider for.
struct Ask<'a> {
    registration: &'a Registration,
    state: String,
    nonce: String,
    challenge: String,
}

/// The most a field of the add form may hold, in characters.
const FIELD_CHARS: usize = 255;

/// What the add form was given, and what is wrong with it.
#[derive(Default)]
struct Draft {
    name: String,
    email: String,
    email_verified: bool,
    subject: String,
    /// Each field's problem, by field.
    problems: Vec<(&'static str, String)>,
    /// A problem with the choice itself, shown above the accounts.
    notice: Option<String>,
}

impl Chooser<'_> {
    /// What the page answers `request` with.
    pub fn answer(&self, request: &Request) -> Response {
        let Some(own) = own_origin(request.header("host")) else {
            return plain(
                421,
                "The sign-in page answers only requests addressed to localhost.",
            );
        };
        let segments: Vec<&str> = request.path.trim_matches('/').split('/').collect();
        let provider = match segments.as_slice() {
            [provider, "authorize"] if !provider.is_empty() => *provider,
            _ => {
                return refused(
                    404,
                    "This address isn't a sign-in",
                    "The local issuer signs in at the address the app's start route gives.",
                );
            }
        };
        let posted = match request.method.as_str() {
            "GET" | "HEAD" => false,
            "POST" => true,
            _ => {
                let mut response = plain(405, "The sign-in page takes GET and POST.");
                response
                    .headers
                    .push(("allow".into(), "GET, HEAD, POST".into()));
                return response;
            }
        };
        if posted && request.header("origin") != Some(own.as_str()) {
            return refused(
                403,
                "This choice didn't come from the sign-in page",
                "Only the page itself can sign in as a test account. Choose the account on the page.",
            );
        }
        let text = if posted {
            String::from_utf8(request.body.clone()).unwrap_or_default()
        } else {
            request.query.clone().unwrap_or_default()
        };
        let Some(values) = query_values(&text) else {
            return refused(
                400,
                "This sign-in can't be read",
                "Start the sign-in again from the app.",
            );
        };
        let one = |name: &str| match values.get(name).map(Vec::as_slice) {
            Some([value]) => value.clone(),
            _ => String::new(),
        };
        let ask = match self.ask(provider, &one) {
            Ok(ask) => ask,
            Err((title, reason)) => return refused(400, title, &reason),
        };
        if !posted {
            let draft = Draft {
                email_verified: true,
                subject: self.new_subject(),
                ..Draft::default()
            };
            return page(&ask, &self.accounts.list(provider), &draft, false, 200);
        }
        if one("choice") == "cancel" {
            let query = format!("error=access_denied&state={}", percent_encode(&ask.state));
            return back(&ask.registration.redirect_uri, &query);
        }
        let account = if values.contains_key("account") {
            let subject = one("account");
            match self
                .accounts
                .list(provider)
                .into_iter()
                .find(|account| account.subject == subject)
            {
                Some(account) => account,
                None => {
                    let draft = Draft {
                        email_verified: true,
                        subject: self.new_subject(),
                        notice: Some(format!(
                            "No test account has the subject {} now. Choose another, or add it again.",
                            code(&subject)
                        )),
                        ..Draft::default()
                    };
                    return page(&ask, &self.accounts.list(provider), &draft, false, 422);
                }
            }
        } else {
            let draft = added(&one);
            if !draft.problems.is_empty() {
                return page(&ask, &self.accounts.list(provider), &draft, true, 422);
            }
            let account = Account {
                subject: draft.subject.clone(),
                email: (!draft.email.is_empty()).then(|| draft.email.clone()),
                email_verified: draft.email_verified,
                name: (!draft.name.is_empty()).then(|| draft.name.clone()),
            };
            self.accounts.keep(provider, &account);
            account
        };
        let registration = ask.registration;
        let code = self.issuer.authorize(
            account,
            &registration.client_id,
            &registration.redirect_uri,
            &ask.nonce,
            &ask.challenge,
            self.now,
        );
        let query = format!(
            "code={}&state={}",
            percent_encode(&code),
            percent_encode(&ask.state)
        );
        back(&registration.redirect_uri, &query)
    }

    /// The sign-in the app's start route asked for, checked as a provider
    /// checks its registered client; else the page's title and why.
    fn ask<'r>(
        &'r self,
        provider: &str,
        one: &dyn Fn(&str) -> String,
    ) -> Result<Ask<'r>, (&'static str, String)> {
        let Some(registration) = self
            .registrations
            .iter()
            .find(|registration| registration.provider == provider)
        else {
            let names: Vec<String> = self
                .registrations
                .iter()
                .map(|registration| code(&registration.provider))
                .collect();
            let reason = match names.as_slice() {
                [] => format!(
                    "{} isn't a sign-in provider of the app, which has none.",
                    code(provider)
                ),
                _ => format!(
                    "{} isn't a sign-in provider of the app, whose providers are {}.",
                    code(provider),
                    names.join(", ")
                ),
            };
            return Err(("This sign-in isn't for the app", reason));
        };
        let (client_id, redirect_uri) = (one("client_id"), one("redirect_uri"));
        if client_id != registration.client_id {
            return Err((
                "This sign-in isn't for the app",
                format!(
                    "It names the client {}, and {}'s is {}.",
                    code(&client_id),
                    code(provider),
                    code(&registration.client_id)
                ),
            ));
        }
        if redirect_uri != registration.redirect_uri {
            return Err((
                "This sign-in can't go back to the app",
                format!(
                    "It names {} as where to return, and {} returns only to {}.",
                    code(&redirect_uri),
                    code(provider),
                    code(&registration.redirect_uri)
                ),
            ));
        }
        let (state, nonce, challenge) = (one("state"), one("nonce"), one("code_challenge"));
        if one("response_type") != "code"
            || one("code_challenge_method") != "S256"
            || state.is_empty()
            || nonce.is_empty()
            || challenge.is_empty()
        {
            return Err((
                "This sign-in is missing what the app sends",
                "A sign-in needs the code flow, a <code>state</code>, a <code>nonce</code> and an <code>S256</code> code challenge, as the app&#39;s start route gives them.".to_string(),
            ));
        }
        Ok(Ask {
            registration,
            state,
            nonce,
            challenge,
        })
    }

    /// A subject for a new account, in the shape Google gives one: a 1
    /// and twenty digits.
    fn new_subject(&self) -> String {
        let mut bytes = [0u8; 16];
        if !(self.random)(&mut bytes) {
            return String::new();
        }
        let number = u128::from_be_bytes(bytes) % 100_000_000_000_000_000_000;
        format!("1{number:020}")
    }
}

/// The add form's fields, checked.
fn added(one: &dyn Fn(&str) -> String) -> Draft {
    let mut draft = Draft {
        name: one("name").trim().to_string(),
        email: one("email").trim().to_string(),
        email_verified: one("email_verified") == "true",
        subject: one("subject").trim().to_string(),
        ..Draft::default()
    };
    if draft.name.chars().count() > FIELD_CHARS {
        draft.problems.push((
            "name",
            format!("Give a name of at most {FIELD_CHARS} characters."),
        ));
    }
    if !draft.email.is_empty()
        && (!jadpo_std::is_email(&draft.email) || draft.email.chars().count() > FIELD_CHARS)
    {
        draft.problems.push((
            "email",
            format!(
                "{} isn't an email address. Give one like {}, or leave it empty.",
                code(&draft.email),
                code("ann@example.com")
            ),
        ));
    }
    if draft.subject.is_empty() {
        draft.problems.push((
            "subject",
            "Give a subject: a provider always sends one.".to_string(),
        ));
    } else if draft.subject.chars().count() > FIELD_CHARS
        || draft.subject.chars().any(char::is_control)
    {
        draft.problems.push((
            "subject",
            format!("Give a subject of at most {FIELD_CHARS} characters, on one line."),
        ));
    }
    draft
}

/// `http://<host>` when `host` names this machine, as the page's own
/// origin; `None` for any other name.
fn own_origin(host: Option<&str>) -> Option<String> {
    let host = host?.trim();
    let name = match host.strip_prefix('[') {
        Some(rest) => rest.split(']').next()?,
        None => host.rsplit_once(':').map_or(host, |(name, _)| name),
    };
    matches!(
        name.to_ascii_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "::1"
    )
    .then(|| format!("http://{}", host.to_ascii_lowercase()))
}

/// `303` to `to` with `query` after it, as the provider sends the browser
/// back.
fn back(to: &str, query: &str) -> Response {
    let separator = if to.contains('?') { '&' } else { '?' };
    Response {
        status: 303,
        headers: vec![
            ("location".into(), format!("{to}{separator}{query}")),
            ("cache-control".into(), "no-store".into()),
            ("referrer-policy".into(), "no-referrer".into()),
        ],
        ..Response::default()
    }
}

fn plain(status: u16, text: &str) -> Response {
    Response {
        status,
        headers: vec![
            ("content-type".into(), "text/plain; charset=utf-8".into()),
            ("cache-control".into(), "no-store".into()),
        ],
        body: format!("{text}\n").into_bytes(),
        ..Response::default()
    }
}

/// `text` as code: escaped, in `<code>`.
fn code(text: &str) -> String {
    format!("<code>{}</code>", esc(text))
}

/// The page saying why a sign-in can't go on; it sends the browser
/// nowhere. `reason` is already HTML.
fn refused(status: u16, title: &str, reason: &str) -> Response {
    let body = format!(
        "<main class=\"refused\"><h1><span class=\"sym\" aria-hidden=\"true\">✗</span>{title}</h1><p>{reason}</p><p class=\"next\">Start the sign-in again from the app.</p></main>",
        title = esc(title),
    );
    html(status, title, &body, "'none'")
}

/// The chooser: the provider's test accounts, the form adding one, and
/// cancelling.
fn page(ask: &Ask<'_>, accounts: &[Account], draft: &Draft, open: bool, status: u16) -> Response {
    let registration = ask.registration;
    let provider = code(&registration.provider);
    let action = format!("/{}/authorize", percent_encode(&registration.provider));
    let hidden = hidden_fields(ask);
    let app = super::signed::origin_of(&registration.redirect_uri).unwrap_or_default();
    let app_host = app.split_once("://").map_or(app.as_str(), |(_, host)| host);
    let mut body = format!(
        "<main><h1>Choose an account</h1><p class=\"to\">to continue to <strong>{app_host}</strong> with {provider}</p><p class=\"note\">The local issuer stands in for {provider}, so these are test accounts kept on this machine.</p>",
        app_host = esc(app_host),
    );
    if let Some(notice) = &draft.notice {
        body.push_str(&format!(
            "<p class=\"notice\" role=\"alert\"><span aria-hidden=\"true\">✗ </span>{notice}</p>"
        ));
    }
    let focus_problem = !draft.problems.is_empty();
    if accounts.is_empty() {
        body.push_str(&format!(
            "<div class=\"empty\"><p>No test accounts for {provider} yet. Add one to sign in as.</p></div><h2 id=\"add-heading\">Add a test account</h2>"
        ));
        body.push_str(&add_form(&action, &hidden, draft, !focus_problem));
    } else {
        body.push_str(&format!(
            "<ul class=\"accounts\" aria-label=\"Test accounts for {name}\">",
            name = esc(&registration.provider)
        ));
        for (at, account) in accounts.iter().enumerate() {
            let focus = at == 0 && !open && draft.notice.is_none();
            body.push_str(&account_row(&action, &hidden, account, focus));
        }
        body.push_str("</ul>");
        body.push_str(&format!(
            "<details{open}><summary>Add a test account</summary>{form}</details>",
            open = if open { " open" } else { "" },
            form = add_form(&action, &hidden, draft, false),
        ));
    }
    body.push_str(&format!(
        "<form class=\"cancel\" method=\"post\" action=\"{action}\">{hidden}<button class=\"button secondary\" type=\"submit\" name=\"choice\" value=\"cancel\">Cancel sign-in</button><p>Answers as a refusal at the provider does, so the app gets <code>sign_in_provider_failed</code>.</p></form><p class=\"returns\">Returns to <span class=\"mono\">{to}</span></p></main>",
        to = esc(&registration.redirect_uri),
    ));
    let title = format!("Sign in with {}", registration.provider);
    html(status, &title, &body, &format!("'self' {app}"))
}

/// The app's request, carried through each form unchanged.
fn hidden_fields(ask: &Ask<'_>) -> String {
    let registration = ask.registration;
    [
        ("response_type", "code"),
        ("client_id", registration.client_id.as_str()),
        ("redirect_uri", registration.redirect_uri.as_str()),
        ("state", ask.state.as_str()),
        ("nonce", ask.nonce.as_str()),
        ("code_challenge", ask.challenge.as_str()),
        ("code_challenge_method", "S256"),
    ]
    .iter()
    .map(|(name, value)| {
        format!(
            "<input type=\"hidden\" name=\"{name}\" value=\"{}\">",
            esc(value)
        )
    })
    .collect()
}

/// One test account, a button that signs in as it.
fn account_row(action: &str, hidden: &str, account: &Account, focus: bool) -> String {
    let title = account
        .name
        .as_deref()
        .or(account.email.as_deref())
        .unwrap_or(&account.subject);
    let initial: String = title
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "?".to_string());
    let address = match &account.email {
        Some(email) if account.email_verified => format!(
            "{} · <span class=\"verified\">✓ verified</span>",
            esc(email)
        ),
        Some(email) => format!(
            "{} · <span class=\"unverified\">– not verified</span>",
            esc(email)
        ),
        None => "no address".to_string(),
    };
    format!(
        "<li><form method=\"post\" action=\"{action}\">{hidden}<button class=\"account\" type=\"submit\" name=\"account\" value=\"{subject}\"{focus}><span class=\"initial\" aria-hidden=\"true\">{initial}</span><span class=\"who\"><span class=\"name\">{title}</span><span class=\"detail\">{address} · sub {subject}</span></span></button></form></li>",
        subject = esc(&account.subject),
        focus = if focus { " autofocus" } else { "" },
        initial = esc(&initial),
        title = esc(title),
    )
}

/// The form adding a test account, and signing in as it.
fn add_form(action: &str, hidden: &str, draft: &Draft, focus_name: bool) -> String {
    let problem = |field: &str| {
        draft
            .problems
            .iter()
            .find(|(name, _)| *name == field)
            .map(|(_, text)| text.as_str())
    };
    let first_problem = draft.problems.first().map(|(name, _)| *name);
    let field = |name: &str,
                 label: &str,
                 kind: &str,
                 value: &str,
                 hint: Option<&str>,
                 extra: &str| {
        let problem = problem(name);
        let mut described = Vec::new();
        if problem.is_some() {
            described.push(format!("{name}-problem"));
        }
        if hint.is_some() {
            described.push(format!("{name}-hint"));
        }
        let focus = first_problem == Some(name) || (focus_name && name == "name");
        format!(
            "<div class=\"field\"><label for=\"{name}\">{label}</label><input type=\"{kind}\" id=\"{name}\" name=\"{name}\" value=\"{value}\" autocomplete=\"off\"{extra}{described}{invalid}{focus}>{problem}{hint}</div>",
            value = esc(value),
            described = if described.is_empty() {
                String::new()
            } else {
                format!(" aria-describedby=\"{}\"", described.join(" "))
            },
            invalid = if problem.is_some() { " aria-invalid=\"true\"" } else { "" },
            focus = if focus { " autofocus" } else { "" },
            problem = problem.map_or_else(String::new, |text| format!(
                "<span class=\"problem\" id=\"{name}-problem\"><span aria-hidden=\"true\">✗ </span>{text}</span>"
            )),
            hint = hint.map_or_else(String::new, |text| format!(
                "<span class=\"hint\" id=\"{name}-hint\">{text}</span>"
            )),
        )
    };
    format!(
        "<form class=\"add\" method=\"post\" action=\"{action}\" novalidate>{hidden}<input type=\"hidden\" name=\"choice\" value=\"add\">{name}{email}<div class=\"field check\"><input type=\"checkbox\" id=\"email_verified\" name=\"email_verified\" value=\"true\"{checked} aria-describedby=\"email_verified-hint\"><label for=\"email_verified\">The provider has verified this address<span class=\"hint\" id=\"email_verified-hint\">Sent as <code>email_verified</code>, which <code>first.address_verified</code> reads.</span></label></div>{subject}<button class=\"button primary\" type=\"submit\">Add and sign in</button></form>",
        name = field("name", "Name", "text", &draft.name, None, ""),
        email = field(
            "email",
            "Email",
            "email",
            &draft.email,
            Some("Leave it empty for an account with no address."),
            " inputmode=\"email\""
        ),
        checked = if draft.email_verified { " checked" } else { "" },
        subject = field(
            "subject",
            "Subject",
            "text",
            &draft.subject,
            Some(
                "The account&#39;s id at the provider, its <code>sub</code>. A first sign-in links it to a user."
            ),
            " class=\"mono\" spellcheck=\"false\""
        ),
    )
}

/// A whole page of `body`, with the page's own headers; its forms may post
/// to `form_action`.
fn html(status: u16, title: &str, body: &str, form_action: &str) -> Response {
    let document = format!(
        "<!doctype html>\n<html lang=\"en-GB\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"color-scheme\" content=\"light dark\"><title>{title} · jadpo dev</title><style>{STYLE}</style></head><body><header class=\"bar\"><span class=\"home\">Sign in</span><span class=\"facts\">jadpo dev · local issuer · no real account is used</span></header>{body}</body></html>\n",
        title = esc(title),
    );
    Response {
        status,
        headers: vec![
            ("content-type".into(), "text/html; charset=utf-8".into()),
            ("cache-control".into(), "no-store".into()),
            // Not `no-referrer`, under which a browser sends the page's
            // own form posts with `Origin: null`, which the page refuses.
            ("referrer-policy".into(), "same-origin".into()),
            ("x-content-type-options".into(), "nosniff".into()),
            (
                "content-security-policy".into(),
                format!(
                    "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action {form_action}; frame-ancestors 'none'"
                ),
            ),
        ],
        body: document.into_bytes(),
        ..Response::default()
    }
}

/// The CLI design language's tokens carried into the browser, as the dev
/// inbox's page carries them.
const STYLE: &str = r#"
:root{color-scheme:light dark;--bg:#ffffff;--panel:#f4f4f5;--line:#e2e3e6;--text:#1d1f23;--muted:#5d6470;--accent:#0b649f;--on-accent:#ffffff;--success:#0f7a55;--failure:#c3262f;--select:#e6f0f7;--code:#eceef1;
--sans:system-ui,-apple-system,"Segoe UI",Roboto,"Helvetica Neue",Arial,sans-serif;--mono:ui-monospace,SFMono-Regular,Menlo,Consolas,"Liberation Mono",monospace}
@media (prefers-color-scheme:dark){:root{--bg:#1e1e1e;--panel:#252628;--line:#34363a;--text:#e4e5e7;--muted:#969da8;--accent:#5cc3f0;--on-accent:#10151a;--success:#4fd69c;--failure:#ff7b70;--select:#1f3340;--code:#2c2e31}}
*{box-sizing:border-box}
html,body{margin:0}
body{background:var(--bg);color:var(--text);font:14px/1.5 var(--sans);-webkit-font-smoothing:antialiased;caret-color:var(--accent);accent-color:var(--accent)}
::selection{background:var(--select)}
code,.mono{font-family:var(--mono);font-size:12.5px;font-variant-numeric:tabular-nums}
code{background:var(--code);padding:1px 5px;border-radius:4px;overflow-wrap:anywhere}
.bar{display:flex;flex-wrap:wrap;align-items:baseline;gap:4px 16px;padding:12px 20px;border-bottom:1px solid var(--line)}
.home{font-weight:650;font-size:15px}
.facts{color:var(--muted)}
main{max-width:480px;margin:0 auto;padding:40px 16px 56px}
h1{font-size:20px;line-height:1.3;font-weight:650;margin:0 0 4px;letter-spacing:-0.01em;text-wrap:balance}
.to{margin:0 0 6px}
.to strong{font-weight:600}
.note{color:var(--muted);margin:0 0 24px;font-size:13px}
.note code{color:var(--text)}
.notice{margin:0 0 16px;color:var(--failure)}
.accounts{list-style:none;margin:0 0 8px;padding:0;border-top:1px solid var(--line)}
.accounts li{border-bottom:1px solid var(--line)}
.accounts form{margin:0}
.account{display:flex;align-items:center;gap:12px;width:100%;padding:12px 8px;margin:0;font:inherit;color:inherit;text-align:left;background:transparent;border:0;border-radius:6px;cursor:pointer;transition:background-color 150ms ease-out,transform 160ms ease-out}
.account:hover{background:var(--panel)}
.account:active{transform:scale(0.99)}
.account:focus-visible{outline:2px solid var(--accent);outline-offset:-2px}
.initial{flex:none;width:32px;height:32px;border-radius:50%;display:grid;place-items:center;background:var(--select);color:var(--accent);font-weight:650}
.who{min-width:0;flex:1}
.name{display:block;font-weight:600;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.detail{display:block;color:var(--muted);font-family:var(--mono);font-size:12px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.verified{color:var(--success)}
.empty{margin:0 0 20px;padding:14px 16px;background:var(--panel);border-radius:6px}
.empty p{margin:0}
details{margin:16px 0 0}
summary{cursor:pointer;font-weight:600;padding:6px 0;width:max-content;border-radius:3px;color:var(--accent)}
summary:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
h2{font-size:14px;font-weight:650;margin:0 0 12px}
.add{margin:12px 0 0;display:grid;gap:14px}
.field label{display:block;font-weight:500;margin:0 0 4px}
.field input[type=text],.field input[type=email]{width:100%;font:inherit;color:var(--text);background:var(--bg);border:1px solid var(--line);border-radius:6px;padding:7px 10px;transition:border-color 150ms ease-out}
.field input.mono{font-family:var(--mono);font-size:13px}
.field input:hover{border-color:var(--muted)}
.field input:focus-visible{outline:2px solid var(--accent);outline-offset:-1px;border-color:var(--accent)}
.field input[aria-invalid=true]{border-color:var(--failure)}
.hint{display:block;color:var(--muted);font-size:12.5px;font-weight:400;margin:4px 0 0}
.problem{display:block;color:var(--failure);font-size:12.5px;margin:4px 0 0}
.check{display:flex;gap:8px;align-items:flex-start}
.check input{margin:3px 0 0;width:16px;height:16px;flex:none}
.check input:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
.check label{font-weight:500;margin:0}
.button{font:inherit;font-weight:600;border-radius:6px;padding:7px 14px;cursor:pointer;border:1px solid transparent;transition:transform 160ms ease-out,background-color 150ms ease-out,border-color 150ms ease-out}
.button:active{transform:scale(0.97)}
.button:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
.primary{background:var(--accent);color:var(--on-accent);justify-self:start}
.primary:hover{background:color-mix(in srgb,var(--accent) 88%,var(--text))}
.secondary{background:var(--bg);color:var(--text);border-color:var(--line);font-weight:500}
.secondary:hover{border-color:var(--muted)}
.cancel{display:flex;flex-wrap:wrap;align-items:center;gap:8px 14px;margin:28px 0 0;padding:20px 0 0;border-top:1px solid var(--line)}
.cancel p{margin:0;color:var(--muted);font-size:12.5px;flex:1;min-width:24ch}
.returns{margin:28px 0 0;color:var(--muted);font-size:12.5px}
.returns .mono{color:var(--text);overflow-wrap:anywhere}
.refused h1{display:flex;gap:8px;align-items:baseline}
.refused .sym{color:var(--failure)}
.refused p{margin:0 0 8px;max-width:60ch}
.refused .next{margin-top:16px}
@media (prefers-reduced-motion:reduce){.account,.button{transition:none}.account:active,.button:active{transform:none}}
"#;

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    const CALLBACK: &str = "http://localhost:3000/auth/google/callback";
    const CLIENT: &str = "desk.apps.example";
    const VERIFIER: &str = "the-pkce-verifier";

    #[derive(Default)]
    struct Kept(RefCell<Vec<(String, Account)>>);

    impl Accounts for Kept {
        fn list(&self, provider: &str) -> Vec<Account> {
            self.0
                .borrow()
                .iter()
                .filter(|(at, _)| at == provider)
                .map(|(_, account)| account.clone())
                .collect()
        }
        fn keep(&self, provider: &str, account: &Account) {
            let mut kept = self.0.borrow_mut();
            kept.retain(|(at, old)| !(at == provider && old.subject == account.subject));
            kept.push((provider.to_string(), account.clone()));
        }
    }

    fn issuer() -> LocalIssuer {
        LocalIssuer::new(&|bytes: &mut [u8]| {
            bytes.fill(7);
            true
        })
        .expect("an issuer")
        .with_page("http://localhost:3002")
    }

    fn registrations() -> Vec<Registration> {
        vec![Registration {
            provider: "google".into(),
            client_id: CLIENT.into(),
            redirect_uri: CALLBACK.into(),
        }]
    }

    fn ann() -> Account {
        Account {
            subject: "104829365172093846251".into(),
            email: Some("ann@desk.example".into()),
            email_verified: true,
            name: Some("Ann Example".into()),
        }
    }

    fn challenge() -> String {
        jadpo_value::encoding::to_base64url(&jadpo_value::hash::sha256(VERIFIER.as_bytes()))
    }

    /// The query the app's start route sends the browser with.
    fn asked(changes: &[(&str, &str)]) -> String {
        let challenge = challenge();
        let mut pairs = vec![
            ("response_type", "code"),
            ("client_id", CLIENT),
            ("redirect_uri", CALLBACK),
            ("scope", "openid email profile"),
            ("state", "the-state"),
            ("nonce", "the-nonce"),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
        ];
        for (name, value) in changes {
            pairs.retain(|(found, _)| found != name);
            if !value.is_empty() {
                pairs.push((name, value));
            }
        }
        pairs
            .iter()
            .map(|(name, value)| format!("{}={}", percent_encode(name), percent_encode(value)))
            .collect::<Vec<_>>()
            .join("&")
    }

    fn get(query: &str) -> Request {
        Request {
            method: "GET".into(),
            path: "/google/authorize".into(),
            query: Some(query.to_string()),
            headers: vec![("host".into(), "localhost:3002".into())],
            ..Request::default()
        }
    }

    fn post(body: &str) -> Request {
        Request {
            method: "POST".into(),
            path: "/google/authorize".into(),
            headers: vec![
                ("host".into(), "localhost:3002".into()),
                ("origin".into(), "http://localhost:3002".into()),
                (
                    "content-type".into(),
                    "application/x-www-form-urlencoded".into(),
                ),
            ],
            body: body.as_bytes().to_vec(),
            ..Request::default()
        }
    }

    fn answer(issuer: &LocalIssuer, kept: &Kept, request: &Request) -> Response {
        let registrations = registrations();
        Chooser {
            issuer,
            registrations: &registrations,
            accounts: kept,
            random: &|bytes: &mut [u8]| {
                bytes.fill(1);
                true
            },
            now: 1_000_000,
        }
        .answer(request)
    }

    fn text(response: &Response) -> String {
        String::from_utf8(response.body.clone()).expect("text")
    }

    /// The code and state a `303` back to the app carries.
    fn returned(response: &Response) -> BTreeMapish {
        assert_eq!(response.status, 303, "{}", text(response));
        let location = response.header("location").expect("a location");
        let (to, query) = location.split_once('?').expect("a query");
        assert_eq!(to, CALLBACK);
        query_values(query).expect("a query")
    }

    type BTreeMapish = std::collections::BTreeMap<String, Vec<String>>;

    fn redeem(issuer: &LocalIssuer, code: &str) -> serde_json::Value {
        let token = issuer
            .redeem(
                code,
                VERIFIER,
                CLIENT,
                CALLBACK,
                "https://provider.invalid",
                1_000_500,
            )
            .expect("the code redeems once");
        let claims = token.split('.').nth(1).expect("claims");
        serde_json::from_slice(&jadpo_value::encoding::from_base64url(claims).expect("base64"))
            .expect("json")
    }

    #[test]
    fn with_no_accounts_the_page_opens_on_adding_one() {
        let (issuer, kept) = (issuer(), Kept::default());
        let page = answer(&issuer, &kept, &get(&asked(&[])));
        assert_eq!(page.status, 200);
        let html = text(&page);
        assert!(html.contains("<html lang=\"en-GB\">"), "{html}");
        assert!(html.contains("<h1>Choose an account</h1>"), "{html}");
        assert!(
            html.contains(
                "to continue to <strong>localhost:3000</strong> with <code>google</code>"
            ),
            "{html}"
        );
        assert!(
            html.contains("No test accounts for <code>google</code> yet"),
            "{html}"
        );
        // Each field has a label, the name is focused, and the subject is
        // made up in the provider's shape.
        for field in ["name", "email", "email_verified", "subject"] {
            assert!(
                html.contains(&format!("<label for=\"{field}\">")),
                "{field}: {html}"
            );
        }
        assert!(
            html.contains("id=\"name\" name=\"name\" value=\"\" autocomplete=\"off\" autofocus"),
            "{html}"
        );
        assert!(html.contains("value=\"1"), "{html}");
        assert!(
            html.contains("name=\"email_verified\" value=\"true\" checked"),
            "{html}"
        );
        assert!(html.contains("Cancel sign-in"), "{html}");
        let policy = page.header("content-security-policy").expect("a policy");
        assert!(
            policy.contains("form-action 'self' http://localhost:3000"),
            "{policy}"
        );
        assert!(policy.contains("frame-ancestors 'none'"), "{policy}");
        assert!(!policy.contains("script-src"), "no script runs: {policy}");
        // A browser sends a form post's real `Origin` only under a policy
        // that lets the origin through.
        assert_eq!(page.header("referrer-policy"), Some("same-origin"));
        assert!(!html.contains("no-referrer"), "{html}");
    }

    #[test]
    fn adding_an_account_keeps_it_and_signs_in_as_it() {
        let (issuer, kept) = (issuer(), Kept::default());
        let body = format!(
            "{}&choice=add&name=Ann%20Example&email=ann%40desk.example&email_verified=true&subject=104829365172093846251",
            asked(&[])
        );
        let back = returned(&answer(&issuer, &kept, &post(&body)));
        assert_eq!(back["state"], ["the-state"]);
        assert_eq!(kept.list("google"), [ann()]);
        let claims = redeem(&issuer, &back["code"][0]);
        assert_eq!(claims["sub"], "104829365172093846251");
        assert_eq!(claims["email"], "ann@desk.example");
        assert_eq!(claims["email_verified"], true);
        assert_eq!(claims["name"], "Ann Example");
        assert_eq!(claims["nonce"], "the-nonce");
        assert_eq!(claims["aud"], CLIENT);
        // Once only.
        assert!(
            issuer
                .redeem(
                    &back["code"][0],
                    VERIFIER,
                    CLIENT,
                    CALLBACK,
                    "https://provider.invalid",
                    1_000_500
                )
                .is_none()
        );
    }

    #[test]
    fn choosing_an_account_signs_in_as_it_and_the_page_lists_it_first_focused() {
        let (issuer, kept) = (issuer(), Kept::default());
        kept.keep("google", &ann());
        let bo = Account {
            subject: "bo-subject".into(),
            email: Some("bo@desk.example".into()),
            email_verified: false,
            name: None,
        };
        kept.keep("google", &bo);
        kept.keep(
            "microsoft",
            &Account {
                subject: "elsewhere".into(),
                email: None,
                email_verified: false,
                name: None,
            },
        );
        let html = text(&answer(&issuer, &kept, &get(&asked(&[]))));
        assert!(
            html.contains("<ul class=\"accounts\" aria-label=\"Test accounts for google\">"),
            "{html}"
        );
        assert!(
            html.contains("value=\"104829365172093846251\" autofocus>"),
            "{html}"
        );
        assert!(
            html.contains("ann@desk.example · <span class=\"verified\">✓ verified</span>"),
            "{html}"
        );
        assert!(
            html.contains("<span class=\"name\">bo@desk.example</span>"),
            "{html}"
        );
        assert!(
            html.contains("<span class=\"unverified\">– not verified</span>"),
            "{html}"
        );
        assert!(
            !html.contains("elsewhere"),
            "another provider's accounts: {html}"
        );
        assert!(
            html.contains("<details><summary>Add a test account</summary>"),
            "{html}"
        );
        let body = format!("{}&account=bo-subject", asked(&[]));
        let back = returned(&answer(&issuer, &kept, &post(&body)));
        let claims = redeem(&issuer, &back["code"][0]);
        assert_eq!(claims["sub"], "bo-subject");
        assert_eq!(claims["email_verified"], false);
        assert!(claims.get("name").is_none());
    }

    #[test]
    fn a_page_made_from_the_same_key_shares_its_codes_with_the_app() {
        let (page, kept) = (issuer(), Kept::default());
        kept.keep("google", &ann());
        let app = LocalIssuer::from_seed(page.seed())
            .expect("the same key")
            .with_codes(page.codes().clone());
        let back = returned(&answer(
            &page,
            &kept,
            &post(&format!("{}&account=104829365172093846251", asked(&[]))),
        ));
        assert_eq!(
            redeem(&app, &back["code"][0])["sub"],
            "104829365172093846251"
        );
        assert_eq!(
            page.authorization("google"),
            "http://localhost:3002/google/authorize"
        );
    }

    #[test]
    fn cancelling_sends_the_providers_refusal_back() {
        let (issuer, kept) = (issuer(), Kept::default());
        let back = returned(&answer(
            &issuer,
            &kept,
            &post(&format!("{}&choice=cancel", asked(&[]))),
        ));
        assert_eq!(back["error"], ["access_denied"]);
        assert_eq!(back["state"], ["the-state"]);
        assert!(!back.contains_key("code"));
    }

    #[test]
    fn a_form_that_doesnt_check_says_why_beside_each_field_and_keeps_what_was_typed() {
        let (issuer, kept) = (issuer(), Kept::default());
        kept.keep("google", &ann());
        let body = format!("{}&choice=add&name=Bo&email=bo%40desk&subject=", asked(&[]));
        let page = answer(&issuer, &kept, &post(&body));
        assert_eq!(page.status, 422);
        let html = text(&page);
        assert!(html.contains("<details open>"), "{html}");
        assert!(html.contains("value=\"bo@desk\""), "{html}");
        assert!(html.contains("value=\"Bo\""), "{html}");
        assert!(
            html.contains(
                "aria-describedby=\"email-problem email-hint\" aria-invalid=\"true\" autofocus"
            ),
            "{html}"
        );
        assert!(
            html.contains("<code>bo@desk</code> isn&#39;t an email address")
                || html.contains("<code>bo@desk</code> isn't an email address"),
            "{html}"
        );
        assert!(html.contains("id=\"subject-problem\""), "{html}");
        assert!(
            html.contains("Give a subject: a provider always sends one."),
            "{html}"
        );
        // Unchecked, the box stays unchecked; nothing is kept.
        assert!(!html.contains("value=\"true\" checked"), "{html}");
        assert_eq!(kept.list("google"), [ann()]);
        // Focus is on the problem, not the first account.
        assert!(
            !html.contains("value=\"104829365172093846251\" autofocus"),
            "{html}"
        );
    }

    #[test]
    fn an_account_no_longer_kept_is_said_so() {
        let (issuer, kept) = (issuer(), Kept::default());
        let page = answer(
            &issuer,
            &kept,
            &post(&format!("{}&account=gone", asked(&[]))),
        );
        assert_eq!(page.status, 422);
        let html = text(&page);
        assert!(html.contains("role=\"alert\""), "{html}");
        assert!(
            html.contains("No test account has the subject <code>gone</code> now"),
            "{html}"
        );
    }

    #[test]
    fn a_sign_in_that_isnt_the_apps_goes_nowhere() {
        let (issuer, kept) = (issuer(), Kept::default());
        kept.keep("google", &ann());
        for (changes, says) in [
            (
                &[("redirect_uri", "http://evil.example/cb")][..],
                "<code>http://evil.example/cb</code> as where to return",
            ),
            (
                &[("client_id", "someone-else")][..],
                "It names the client <code>someone-else</code>",
            ),
            (
                &[("code_challenge", "")][..],
                "is missing what the app sends",
            ),
            (
                &[("code_challenge_method", "plain")][..],
                "is missing what the app sends",
            ),
            (&[("state", "")][..], "is missing what the app sends"),
            (
                &[("response_type", "token")][..],
                "is missing what the app sends",
            ),
        ] {
            for request in [
                get(&asked(changes)),
                post(&format!("{}&account=104829365172093846251", asked(changes))),
            ] {
                let page = answer(&issuer, &kept, &request);
                assert_eq!(page.status, 400, "{changes:?}");
                assert!(page.header("location").is_none(), "{changes:?}");
                let html = text(&page);
                assert!(html.contains(says), "{changes:?}: {html}");
                assert!(
                    html.contains("form-action 'none'")
                        || page
                            .header("content-security-policy")
                            .is_some_and(|policy| policy.contains("form-action 'none'"))
                );
            }
        }
        let mut other = get(&asked(&[]));
        other.path = "/github/authorize".into();
        let page = answer(&issuer, &kept, &other);
        assert_eq!(page.status, 400);
        assert!(text(&page).contains("<code>github</code> isn&#39;t a sign-in provider of the app, whose providers are <code>google</code>") || text(&page).contains("<code>github</code> isn't a sign-in provider of the app, whose providers are <code>google</code>"), "{}", text(&page));
    }

    #[test]
    fn only_this_machine_and_the_page_itself_are_answered() {
        let (issuer, kept) = (issuer(), Kept::default());
        kept.keep("google", &ann());
        let choose = format!("{}&account=104829365172093846251", asked(&[]));
        // Another site's form.
        let mut foreign = post(&choose);
        foreign.headers[1].1 = "http://evil.example".into();
        assert_eq!(answer(&issuer, &kept, &foreign).status, 403);
        let mut none = post(&choose);
        none.headers.remove(1);
        assert_eq!(answer(&issuer, &kept, &none).status, 403);
        // A name pointed at this machine from elsewhere.
        let mut rebound = get(&asked(&[]));
        rebound.headers[0].1 = "evil.example:3002".into();
        assert_eq!(answer(&issuer, &kept, &rebound).status, 421);
        for host in ["127.0.0.1:3002", "[::1]:3002", "LOCALHOST:3002"] {
            let mut local = get(&asked(&[]));
            local.headers[0].1 = host.into();
            assert_eq!(answer(&issuer, &kept, &local).status, 200, "{host}");
        }
        let mut elsewhere = get(&asked(&[]));
        elsewhere.path = "/".into();
        assert_eq!(answer(&issuer, &kept, &elsewhere).status, 404);
        let mut put = post(&choose);
        put.method = "PUT".into();
        assert_eq!(answer(&issuer, &kept, &put).status, 405);
    }
}
