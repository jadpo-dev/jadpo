//! SpamAssassin's rules in the dev inbox's health score (area 16, "A mail
//! health score"; chunk 7.3c3). When `spamassassin` is installed, `jadpo
//! dev` runs each mail it catches through it, as the SMTP driver writes
//! the mail, before keeping it, and keeps what it found beside the mail:
//! the score, the score it calls spam at, and each rule that matched.
//! `jadpo email show` and the inbox page show them after Jadpo's own
//! checks. When it isn't installed, or couldn't score a mail, they say so,
//! never that the mail is clean.
//!
//! Its run goes through [`SpamFilter`], which the tests fake; the real one
//! ([`SystemSpamAssassin`]) runs it on this machine with its network tests
//! off ([`jadpo_host_native::spamassassin`]).

use jadpo_runtime::LocalFuture;
use jadpo_value::Value;

/// What SpamAssassin made of one mail.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Spam {
    /// Kept without a result: caught before SpamAssassin was run on mail.
    #[default]
    NotRun,
    /// It isn't installed: no `spamassassin` on the `PATH`.
    NotInstalled,
    /// It ran and gave no report, for this reason.
    Failed(String),
    /// Its report.
    Ran(Report),
}

/// SpamAssassin's report on a mail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Report {
    /// Its score, as it wrote it: `2.3`.
    pub score: String,
    /// The score it calls a mail spam at: `5.0`.
    pub required: String,
    /// Each rule that matched, the most points first.
    pub rules: Vec<Rule>,
}

/// One rule that matched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Rule {
    /// `MISSING_DATE`.
    pub name: String,
    /// Its points, as SpamAssassin wrote them: `1.4`, `-0.0`.
    pub points: String,
    /// What it matched, in SpamAssassin's words.
    pub description: String,
}

impl Rule {
    /// Whether it counts against the mail.
    pub fn counts(&self) -> bool {
        number(&self.points) > 0.0
    }
}

impl Report {
    /// Whether SpamAssassin calls the mail spam.
    pub fn is_spam(&self) -> bool {
        number(&self.score) >= number(&self.required)
    }
}

impl Spam {
    /// Whether SpamAssassin calls the mail spam, which counts as one more
    /// problem in its health.
    pub fn is_spam(&self) -> bool {
        matches!(self, Spam::Ran(report) if report.is_spam())
    }

    /// How a score's summary line names it: `SpamAssassin 2.3 points`.
    pub fn summary(&self) -> String {
        match self {
            Spam::NotRun => "SpamAssassin wasn't run on it".to_string(),
            Spam::NotInstalled => "SpamAssassin wasn't run: it isn't installed".to_string(),
            Spam::Failed(_) => "SpamAssassin couldn't score it".to_string(),
            Spam::Ran(report) if report.is_spam() => format!(
                "SpamAssassin calls it spam: {} points, spam from {}",
                report.score, report.required
            ),
            Spam::Ran(report) => format!(
                "SpamAssassin {} points, spam from {}",
                report.score, report.required
            ),
        }
    }

    /// A few words for a line that has little room: `SpamAssassin 2.3
    /// of 5.0`.
    pub fn short(&self) -> String {
        match self {
            Spam::NotRun | Spam::NotInstalled => "SpamAssassin not run".to_string(),
            Spam::Failed(_) => "SpamAssassin gave no report".to_string(),
            Spam::Ran(report) if report.is_spam() => {
                format!(
                    "SpamAssassin: spam, {} of {}",
                    report.score, report.required
                )
            }
            Spam::Ran(report) => format!("SpamAssassin {} of {}", report.score, report.required),
        }
    }

    /// Why it has no report, and what to do about it, when it has none.
    pub fn absent(&self) -> Option<(String, String)> {
        match self {
            Spam::Ran(_) => None,
            Spam::NotRun => Some((
                "This mail was caught before SpamAssassin was run on mail, so its rules aren't in this score.".to_string(),
                "Send the mail again while `jadpo dev` runs".to_string(),
            )),
            Spam::NotInstalled => Some((
                "SpamAssassin isn't installed, so its rules aren't in this score. That doesn't make the mail clean.".to_string(),
                "Install SpamAssassin to add them: `jadpo dev` runs each mail it catches through it, on this machine, with its network tests off".to_string(),
            )),
            Spam::Failed(why) => Some((
                format!("SpamAssassin is installed but gave no report: {why}. Its rules aren't in this score."),
                "Run `spamassassin --lint` to check its setup, then send the mail again".to_string(),
            )),
        }
    }

    /// As the inbox keeps it beside the mail.
    pub fn to_json(&self) -> Option<serde_json::Value> {
        Some(match self {
            Spam::NotRun => return None,
            Spam::NotInstalled => serde_json::json!({"state": "not_installed"}),
            Spam::Failed(why) => serde_json::json!({"state": "failed", "why": why}),
            Spam::Ran(report) => serde_json::json!({
                "state": "ran",
                "score": report.score,
                "required": report.required,
                "rules": report.rules.iter().map(|rule| serde_json::json!({
                    "name": rule.name,
                    "points": rule.points,
                    "description": rule.description,
                })).collect::<Vec<_>>(),
            }),
        })
    }

    pub fn from_json(json: Option<&serde_json::Value>) -> Spam {
        let Some(json) = json else {
            return Spam::NotRun;
        };
        let text = |value: &serde_json::Value, name: &str| {
            value
                .get(name)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        match json.get("state").and_then(serde_json::Value::as_str) {
            Some("not_installed") => Spam::NotInstalled,
            Some("failed") => Spam::Failed(text(json, "why")),
            Some("ran") => Spam::Ran(Report {
                score: text(json, "score"),
                required: text(json, "required"),
                rules: json
                    .get("rules")
                    .and_then(serde_json::Value::as_array)
                    .map(|rules| {
                        rules
                            .iter()
                            .map(|rule| Rule {
                                name: text(rule, "name"),
                                points: text(rule, "points"),
                                description: text(rule, "description"),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            }),
            _ => Spam::NotRun,
        }
    }

    /// As `jadpo email show --json` gives it.
    pub fn to_output(&self) -> jadpo_term::Json {
        use jadpo_term::Json;
        match self {
            Spam::Ran(report) => Json::object([
                ("ran", Json::from(true)),
                ("score", decimal(&report.score)),
                ("required", decimal(&report.required)),
                ("spam", Json::from(report.is_spam())),
                (
                    "rules",
                    Json::array(report.rules.iter().map(|rule| {
                        Json::object([
                            ("rule", Json::from(rule.name.as_str())),
                            ("points", decimal(&rule.points)),
                            ("description", Json::from(rule.description.as_str())),
                        ])
                    })),
                ),
            ]),
            other => Json::object([
                ("ran", Json::from(false)),
                (
                    "why",
                    Json::from(match other {
                        Spam::NotInstalled => "not_installed",
                        Spam::Failed(_) => "failed",
                        _ => "not_run",
                    }),
                ),
                (
                    "detail",
                    match other {
                        Spam::Failed(why) => Json::from(why.as_str()),
                        _ => Json::Null,
                    },
                ),
            ]),
        }
    }
}

fn number(text: &str) -> f64 {
    text.trim().parse::<f64>().unwrap_or(0.0)
}

/// A score as SpamAssassin wrote it, a decimal kept as text, as Jadpo's
/// JSON writes decimals.
fn decimal(text: &str) -> jadpo_term::Json {
    jadpo_term::Json::from(text)
}

/// The report at the end of what `spamassassin -t` printed: the last
/// "Content analysis details" block, since a mail's own words come before
/// it. Each rule's continuation lines join its description; a bracketed
/// line, which quotes the mail (`[URIs: …]`), is left out.
pub(crate) fn parse(out: &str) -> Option<Report> {
    let lines: Vec<&str> = out.lines().collect();
    let at = lines
        .iter()
        .rposition(|line| line.trim_start().starts_with("Content analysis details:"))?;
    let heading = lines.get(at)?;
    let inside = heading.split_once('(')?.1.split_once(')')?.0;
    let (score, required) = inside.split_once(',')?;
    let score = score.trim().strip_suffix("points")?.trim().to_string();
    let required = required.trim().strip_suffix("required")?.trim().to_string();
    if score.parse::<f64>().is_err() || required.parse::<f64>().is_err() {
        return None;
    }
    let mut rules: Vec<Rule> = Vec::new();
    let mut table = false;
    for line in lines.iter().skip(at + 1) {
        if !table {
            table = line.starts_with("----");
            continue;
        }
        if line.trim().is_empty() {
            break;
        }
        let mut words = line.split_whitespace();
        let first = words.next().unwrap_or_default();
        let row = first.parse::<f64>().is_ok() && !line.starts_with("     ");
        if row {
            let Some(name) = words.next() else { break };
            rules.push(Rule {
                name: name.to_string(),
                points: first.to_string(),
                description: words.collect::<Vec<_>>().join(" "),
            });
        } else if let Some(rule) = rules.last_mut() {
            let more = line.trim();
            if !more.starts_with('[') {
                if !rule.description.is_empty() {
                    rule.description.push(' ');
                }
                rule.description.push_str(more);
            }
        }
    }
    // The most points first; SpamAssassin's own order otherwise.
    rules.sort_by(|a, b| {
        number(&b.points)
            .partial_cmp(&number(&a.points))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Some(Report {
        score,
        required,
        rules,
    })
}

/// Where the dev inbox has a mail scored.
pub(crate) trait SpamFilter {
    /// What SpamAssassin makes of `message`, a mail as the SMTP driver
    /// writes it.
    fn score(&self, message: Vec<u8>) -> LocalFuture<'_, Spam>;
}

/// `spamassassin` on this machine's `PATH`, looked for on each mail, so
/// installing it during a session takes effect.
pub(crate) struct SystemSpamAssassin;

impl SpamFilter for SystemSpamAssassin {
    fn score(&self, message: Vec<u8>) -> LocalFuture<'_, Spam> {
        let found = jadpo_host_native::spamassassin::find(std::env::var_os("PATH").as_deref());
        Box::pin(async move {
            let Some(program) = found else {
                return Spam::NotInstalled;
            };
            match jadpo_host_native::spamassassin::run_later(program, message).await {
                Ok(out) => match parse(&out) {
                    Some(report) => Spam::Ran(report),
                    None => Spam::Failed("its report couldn't be read".to_string()),
                },
                Err(error) => Spam::Failed(error.to_string()),
            }
        })
    }
}

/// `message`, the record the runtime sends, as the SMTP driver writes it,
/// dated `at_ms`, with the `Message-ID:` a submission server adds (RFC
/// 6409, section 8.3), so SpamAssassin scores the mail a receiver gets.
pub(crate) fn as_sent(message: &Value, at_ms: i64) -> Option<Vec<u8>> {
    let envelope = jadpo_drivers::smtp::compose(message, Some(at_ms)).ok()?;
    let domain = envelope
        .from
        .rsplit_once('@')
        .map_or("localhost", |(_, domain)| domain);
    let mut out = format!("Message-ID: <{at_ms}.jadpo-dev@{domain}>\r\n").into_bytes();
    out.extend_from_slice(&envelope.data);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `spamassassin -L -t` prints, its report after the mail.
    const OUT: &str = "Message-ID: <1.jadpo-dev@notes.example>\r
Subject: Content analysis details:   (99.0 points, 1.0 required)\r
\r
Hello\r
\r
Spam detection software, running on the system \"dev.local\",
has NOT identified this incoming email as spam.

Content preview:  Hello [...]

Content analysis details:   (1.6 points, 5.0 required)

 pts rule name              description
---- ---------------------- --------------------------------------------------
-0.0 NO_RELAYS              Informational: message was not relayed via SMTP
 0.1 MISSING_MID            Missing Message-Id: header
 1.5 HTML_IMAGE_ONLY_08     BODY: HTML: images with 400-800 bytes of
                            words
                            [URIs: notes.example]
 0.0 HTML_MESSAGE           BODY: HTML included in message

";

    #[test]
    fn the_last_report_is_read_its_rules_most_points_first() {
        let report = parse(OUT).expect("a report");
        assert_eq!(report.score, "1.6");
        assert_eq!(report.required, "5.0");
        assert!(!report.is_spam());
        assert_eq!(
            report
                .rules
                .iter()
                .map(|rule| (rule.points.as_str(), rule.name.as_str()))
                .collect::<Vec<_>>(),
            [
                ("1.5", "HTML_IMAGE_ONLY_08"),
                ("0.1", "MISSING_MID"),
                ("-0.0", "NO_RELAYS"),
                ("0.0", "HTML_MESSAGE"),
            ]
        );
        assert_eq!(
            report.rules[0].description,
            "BODY: HTML: images with 400-800 bytes of words"
        );
        assert!(report.rules[0].counts());
        assert!(!report.rules[2].counts());
        assert_eq!(parse("Nothing here\n"), None);
        assert_eq!(
            parse("Content analysis details:   (lots points, 5.0 required)\n"),
            None
        );
    }

    #[test]
    fn a_result_is_kept_and_read_back_whole() {
        let ran = Spam::Ran(parse(OUT).expect("a report"));
        for spam in [
            ran,
            Spam::NotInstalled,
            Spam::Failed("it exited with status 2".to_string()),
        ] {
            assert_eq!(Spam::from_json(spam.to_json().as_ref()), spam);
        }
        assert_eq!(Spam::from_json(None), Spam::NotRun);
        assert_eq!(Spam::NotRun.to_json(), None);
    }

    #[test]
    fn a_score_at_its_threshold_is_spam() {
        let spam = Spam::Ran(Report {
            score: "5.0".to_string(),
            required: "5.0".to_string(),
            rules: Vec::new(),
        });
        assert!(spam.is_spam());
        assert_eq!(
            spam.summary(),
            "SpamAssassin calls it spam: 5.0 points, spam from 5.0"
        );
        assert!(!Spam::NotInstalled.is_spam());
        assert!(
            Spam::NotInstalled
                .absent()
                .is_some_and(|(why, _)| why.contains("doesn't make the mail clean"))
        );
    }

    #[test]
    fn a_mail_is_written_as_the_smtp_driver_sends_it_with_a_message_id() {
        let message = Value::record(
            jadpo_std::email::MESSAGE,
            jadpo_std::email::MESSAGE_FIELDS
                .iter()
                .zip([
                    Value::text("hello@notes.example"),
                    Value::text("ada@notes.example"),
                    Value::text("Welcome, Ada"),
                    Value::None,
                    Value::text("Hello"),
                    Value::List(Vec::new().into()),
                ])
                .map(|(name, value)| (std::rc::Rc::from(*name), value))
                .collect(),
        );
        let sent = String::from_utf8(as_sent(&message, 0).expect("written")).expect("text");
        assert!(
            sent.starts_with("Message-ID: <0.jadpo-dev@notes.example>\r\nDate: Thu, 01 Jan 1970"),
            "{sent}"
        );
        assert!(sent.contains("Subject: Welcome, Ada\r\n"), "{sent}");
        assert_eq!(as_sent(&Value::Unit, 0), None);
    }
}
