//! The dev inbox (area 16, "Email"; chunk 7.3c2): `jadpo dev` catches
//! every mail the app sends whole, nothing leaving the machine, and an
//! agent follows it from the command line while a person reads it on the
//! inbox's page:
//!
//! - a route that emits an email has it caught, its SMTP connection's
//!   password never given, and `jadpo dev`'s line for it names its id and
//!   address, never its words;
//! - `jadpo email` lists it, shows it with its links and health, opens a
//!   link in the browser `$BROWSER` names, and clears the inbox;
//! - the page shows it, and its HTML on its own address;
//! - SpamAssassin, when installed, scored it as it was caught; otherwise
//!   its score says SpamAssassin wasn't run (chunk 7.3c3).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use jadpo_cli::run;
use jadpo_term::Environment;

use super::http::{Dev, jadpo, tempdir, write};

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"application {
    display_name: "Notes"
    connections: {
        mail: {
            driver: Driver.smtp
            host: config.smtp_host
            credential: {
                username: config.smtp_username
                password: config.smtp_password
            }
        }
    }
    email: {
        connection: mail
        from: config.mail_sender
    }
    events: {
        store: main
    }
}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    app_url: Url {
        default: "http://localhost:5173"
    }
    mail_sender: Email {
        default: "hello@notes.example"
    }
    smtp_host: Hostname
    smtp_password: Text {
        secret: true
    }
    smtp_username: Text
}
"#,
    ),
    (
        "domains/members/types/member_name.jadpo",
        "/// What a member is called.\ntype MemberName = Text {\n    min_length: 1\n    max_length: 80\n}\n",
    ),
    (
        "domains/members/entities/member.jadpo",
        r#"use members/types { MemberName }

/// Someone who has joined.
entity Member {
    id: Uuid
    address: Email
    name: MemberName

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
"#,
    ),
    (
        "domains/members/types/join_form.jadpo",
        r#"use members/types { MemberName }

/// What joining takes.
type JoinForm = Input {
    address: Email
    name: MemberName
}
"#,
    ),
    (
        "domains/members/types/joined.jadpo",
        r#"use members/types { MemberName }

/// Who joined.
type Joined = Output {
    name: MemberName
}
"#,
    ),
    (
        "domains/members/emails/welcome_email.jadpo",
        r#"use members/types { MemberName }

/// Sent once someone joins.
type WelcomeEmail = EmailMessage {
    name: MemberName
    start_link: Url
}
"#,
    ),
    (
        "domains/members/emails/welcome_email.html",
        "<p>Hello, {{ name }}.</p>\n<p><a href=\"{{ start_link }}\">Start</a></p>\n",
    ),
    (
        "domains/members/emails/welcome_email.txt",
        "Hello, {{ name }}.\n\nStart: {{ start_link }}\n",
    ),
    (
        "domains/members/translations/en_gb.jadpo",
        "translations {\n    welcome_email: \"Welcome, {name}\"\n}\n",
    ),
    (
        "domains/members/actions/join.jadpo",
        r#"use members/entities { Member }
use members/emails { WelcomeEmail }
use members/types { JoinForm, Joined }

/// Signs someone up, and welcomes them once it is done.
action join(input: JoinForm) -> Joined {
    var member: Member = Member.create {
        set: {
            address: input.address
            name: input.name
        }
    }
    emit WelcomeEmail {
        to: member.address
        locale: Locale.en_gb
        name: member.name
        start_link: config.app_url
    }
    return Joined {
        name: member.name
    }
}
"#,
    ),
    (
        "domains/members/routes/members.jadpo",
        r#"use members/types { JoinForm, Joined }
use members/actions { join }

routes Members {
    prefix: /members

    /// Joins.
    POST / {
        authentication: RouteAuthentication.none
        input: JoinForm
        output: Joined
        run: join(input)
    }
}
"#,
    ),
    (
        "policy.jadpo",
        "use members\n\npolicy {\n    actions: {\n        members.join: [Access.public]\n    }\n}\n",
    ),
];

fn project() -> (tempdir::Dir, PathBuf) {
    let dir = tempdir::Dir::new("dev-inbox");
    let root = dir.path().join("notes");
    for (path, text) in FILES {
        write(&root, path, text);
    }
    for args in [
        &["migrate", "new", "initial"][..],
        &["records", "update"],
        &["check"],
    ] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    }
    (dir, root)
}

fn json(root: &Path, args: &[&str]) -> serde_json::Value {
    let mut args = args.to_vec();
    args.push("--json");
    let done = jadpo(root, &args);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    serde_json::from_str::<serde_json::Value>(&done.stdout).expect("json")["data"].clone()
}

/// Waits for `ready` to hold, at most 30 seconds.
fn eventually(what: &str, ready: impl Fn() -> bool) {
    let started = Instant::now();
    while !ready() {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_mail_the_app_sends_in_dev_is_caught_listed_shown_opened_and_cleared() {
    if !jadpo_test::live() {
        return;
    }
    let (dir, root) = project();
    // `smtp_password`, `smtp_host` and `smtp_username` are set nowhere:
    // the inbox catches every mail, so `jadpo dev` starts without them.
    let dev = Dev::start(&root);
    let joined = dev.send(
        "POST",
        "/members",
        &[],
        r#"{"address": "ada@notes.example", "name": "Ada"}"#,
    );
    assert_eq!(joined.status, 200, "{joined:?}");
    eventually("the mail is caught", || {
        json(&root, &["email"])["mails"]
            .as_array()
            .is_some_and(|mails| mails.len() == 1)
    });
    let listed = json(&root, &["email"]);
    assert_eq!(listed["mails"][0]["id"], "mail_1");
    assert_eq!(listed["mails"][0]["to"], "ada@notes.example");
    assert_eq!(listed["mails"][0]["subject"], "Welcome, Ada");
    assert_eq!(listed["mails"][0]["problems"], 0);
    let shown = json(&root, &["email", "show", "mail_1"]);
    assert_eq!(shown["email"], "members.WelcomeEmail");
    assert_eq!(shown["from"], "hello@notes.example");
    assert_eq!(
        shown["text"],
        "Hello, Ada.\n\nStart: http://localhost:5173\n"
    );
    assert_eq!(shown["links"], serde_json::json!(["http://localhost:5173"]));
    assert_eq!(shown["health"]["checks"], 7);
    assert_eq!(shown["health"]["problems"], serde_json::json!([]));
    assert!(
        shown["health"]["note"]
            .as_str()
            .is_some_and(|note| note.contains("can't predict")),
        "{shown}"
    );
    assert_eq!(shown["trace"].as_str().map(str::len), Some(32), "{shown}");
    let installed =
        jadpo_host_native::spamassassin::find(std::env::var_os("PATH").as_deref()).is_some();
    let spam = &shown["health"]["spamassassin"];
    if installed {
        assert!(spam["ran"].is_boolean(), "{shown}");
    } else {
        assert_eq!(spam["ran"], false, "{shown}");
        assert_eq!(spam["why"], "not_installed", "{shown}");
    }
    let links = json(&root, &["email", "links", "1"]);
    assert_eq!(links["links"], serde_json::json!(["http://localhost:5173"]));
    // `jadpo dev`'s line names the mail, never its words.
    eventually("the mail's line", || {
        dev.log().contains("\"event\":\"mail\"")
    });
    let log = dev.log();
    assert!(log.contains("\"id\":\"mail_1\""), "{log}");
    assert!(!log.contains("Welcome, Ada"), "{log}");
    assert!(!log.contains("Hello, Ada"), "{log}");
    // The page.
    let page = dev.inbox("/");
    assert_eq!(page.status, 200);
    assert!(page.body().contains("Welcome, Ada"), "{}", page.body());
    assert!(
        page.body().contains("Health: all 7 checks pass"),
        "{}",
        page.body()
    );
    if !installed {
        assert!(
            page.body().contains("· SpamAssassin not run"),
            "{}",
            page.body()
        );
    }
    let body = dev.inbox("/mail/mail_1/body");
    assert!(body.body().contains("Hello, Ada."), "{}", body.body());
    assert!(
        body.header("content-security-policy")
            .is_some_and(|policy| policy.starts_with("sandbox")),
        "{body:?}"
    );
    // Opening a link starts `$BROWSER` with it.
    let opened = dir.path().join("opened.txt");
    let browser = dir.path().join("browser.sh");
    std::fs::write(
        &browser,
        format!("#!/bin/sh\nprintf '%s' \"$1\" > '{}'\n", opened.display()),
    )
    .expect("written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&browser, std::fs::Permissions::from_mode(0o755))
            .expect("executable");
    }
    let args: Vec<String> = ["email", "open", "mail_1", "1"]
        .iter()
        .map(|arg| arg.to_string())
        .collect();
    let open = run(
        &args,
        &Environment::empty()
            .with_var("BROWSER", &browser.display().to_string())
            .with_cwd(&root),
    );
    assert_eq!(open.code, 0, "{}{}", open.stdout, open.stderr);
    eventually("the browser is started", || {
        std::fs::read_to_string(&opened).is_ok_and(|text| text == "http://localhost:5173")
    });
    let cleared = json(&root, &["email", "clear"]);
    assert_eq!(cleared["cleared"], 1);
    assert_eq!(json(&root, &["email"])["mails"], serde_json::json!([]));
    drop(dev);
}

#[test]
fn an_unknown_mail_or_link_names_the_nearest() {
    let dir = tempdir::Dir::new("dev-inbox-errors");
    let root = dir.path().join("notes");
    write(&root, "app.jadpo", "application {}\n");
    let missing = jadpo(&root, &["email", "show", "mail_4"]);
    assert_eq!(missing.code, 2, "{}", missing.stdout);
    assert!(
        missing.stdout.contains("There is no mail `mail_4`")
            || missing.stderr.contains("There is no mail `mail_4`"),
        "{}{}",
        missing.stdout,
        missing.stderr
    );
    let empty = jadpo(&root, &["email"]);
    assert_eq!(empty.code, 0);
    assert!(empty.stdout.contains("No mail yet"), "{}", empty.stdout);
    let wrong = jadpo(&root, &["email", "shwo"]);
    assert!(
        format!("{}{}", wrong.stdout, wrong.stderr).contains("Did you mean `show`?"),
        "{}{}",
        wrong.stdout,
        wrong.stderr
    );
}

#[test]
fn check_scores_templates_and_a_decline_keeps_one_on_purpose() {
    let dir = tempdir::Dir::new("health-check");
    let root = dir.path().join("notes");
    write(
        &root,
        "app.jadpo",
        "application {\n    display_name: \"Notes\"\n    connections: {\n        mail: {\n            driver: Driver.smtp\n            host: \"smtp.notes.example\"\n        }\n    }\n    email: {\n        connection: mail\n        from: \"hello@notes.example\"\n    }\n}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb]\n}\n",
    );
    write(
        &root,
        "domains/accounts/translations/en_gb.jadpo",
        "translations {\n    sale_email: \"BIG SALE FOR {name}\"\n}\n",
    );
    write(
        &root,
        "domains/accounts/emails/sale_email.html",
        "<div><b>Hello, {{ name }}.</div>\n<a href=\"https://bit.ly/sale\">Shop</a>\n",
    );
    let declared = "type SaleEmail = EmailMessage {\n    name: Text\n}\n";
    write(
        &root,
        "domains/accounts/emails/sale_email.jadpo",
        &format!("/// Our sale.\n{declared}"),
    );
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check", "--json"]);
    let found: Vec<String> = serde_json::from_str::<serde_json::Value>(&checked.stdout)
        .expect("json")["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter(|d| d["rule"] == "email.health_problem")
        .map(|d| d["message"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        found,
        [
            "`SaleEmail` has no plain-text part",
            "`SaleEmail` links through a shortener",
            "`SaleEmail`'s subject is in capitals",
            "`sale_email.html` doesn't parse cleanly",
        ],
        "{}",
        checked.stdout
    );
    write(
        &root,
        "domains/accounts/emails/sale_email.jadpo",
        &format!(
            "/// Our sale.\ndecline email.health_problem: \"A test of what the score finds\"\n{declared}"
        ),
    );
    jadpo(&root, &["records", "update"]);
    let declined = jadpo(&root, &["check"]);
    assert_eq!(declined.code, 0, "{}{}", declined.stdout, declined.stderr);
}

/// `jadpo dev` keeps `build/`, where its inbox and call log live, out of
/// git: it adds `build/` to `.gitignore` when it starts, once, keeping
/// what the file held (area 14, "The `build/` folder"; Phase 7 review,
/// chunk 7.9c).
#[test]
fn dev_keeps_build_out_of_git() {
    let (_dir, root) = project();
    write(&root, ".gitignore", ".env.local");
    let started = |root: &Path| {
        let args = jadpo_cli::dev::DevArgs {
            cwd: root.to_path_buf(),
            port: 0,
            url: None,
            no_run: true,
            json: false,
            choice: jadpo_term::ColorChoice::Never,
            fake: Vec::new(),
            sandbox: Vec::new(),
        };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        jadpo_cli::dev::serve_to(&args, &Environment::empty(), &mut out, &mut err, &|| true);
    };
    started(&root);
    started(&root);
    let ignored = std::fs::read_to_string(root.join(".gitignore")).expect(".gitignore");
    assert_eq!(ignored, ".env.local\nbuild/\n");
}
