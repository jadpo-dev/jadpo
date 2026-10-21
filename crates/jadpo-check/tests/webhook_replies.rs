//! Webhooks that need a reply (area 11, "Webhooks that need a reply";
//! Phase 7 review, chunk 7.9b): the action a connection's `webhooks: {
//! replies: }` names runs as the system, so what it reaches is let in by
//! `Access.system` (`policy.system_grant_missing`), and its reply is sent
//! to whoever sent the webhook, an output to `Access.public`, held to area
//! 9's output proof (`policy.output_field_unproved`).

use crate::support;

use jadpo_db::Layout;

const HOME: &str = "shared/providers/acme_voice/v1";

const APP: &str = "use support\n\napplication {\n    connections: {\n        phone: {\n            package: AcmeVoice.v1\n            domains: [support]\n            credential: config.phone_key\n            host: config.phone_address\n            webhooks: {\n                secret: config.phone_auth_token\n                replies: {\n                    incoming_call: support.answer_call\n                }\n            }\n        }\n    }\n}\n";

const CONFIG: &str = "config {\n    phone_address: Url\n    phone_auth_token: Text {\n        secret: true\n    }\n    phone_key: Text {\n        secret: true\n    }\n}\n";

const PACKAGE: &[(&str, &str)] = &[
    (
        "package.jadpo",
        "/// Acme Voice's calls API.\npackage AcmeVoice.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
    ),
    (
        "provider.jadpo",
        "/// Acme Voice's API, which signs its webhooks as Twilio does.\nprovider {\n    protocol: Protocol.json_http\n    credential: CredentialScheme.bearer\n    webhooks: {\n        check: SignatureScheme.twilio\n        event_id: \"CallSid\"\n        event_type: \"CallStatus\"\n    }\n}\n",
    ),
    (
        "types/call_reply.jadpo",
        "/// What a caller hears.\npublic type CallReply = Object {\n    spoken: Text\n    hang_up: Bool {\n        wire_name: \"hangUp\"\n    }\n}\n",
    ),
    (
        "events/incoming_call.jadpo",
        "/// A call is ringing, and waits to be told what to do.\npublic type IncomingCall = Event {\n    wire_name: \"ringing\"\n    reply: CallReply\n    occurred_at: Instant\n    account_id: Uuid {\n        wire_name: \"AccountSid\"\n    }\n    call_sid: Text {\n        wire_name: \"CallSid\"\n    }\n    from: Text {\n        wire_name: \"From\"\n    }\n}\n",
    ),
];

/// A call the desk answered, with what the desk noted of the caller,
/// which only the desk's own work reads; `create` is who may create one.
fn phone_log(create: &str) -> String {
    format!(
        "/// A call the desk answered.\nentity PhoneLog {{\n    id: Uuid\n    call_sid: Text\n    caller: Text\n    rang_at: Instant\n    secret_note: Text\n\n    policy: {{\n        create: [{create}]\n        read: [Access.system]\n    }}\n}}\n"
    )
}

const EARLIER: (&str, &str) = (
    "domains/support/queries/earlier_calls.jadpo",
    "use support/entities { PhoneLog }\n\n/// The last calls the desk answered.\nquery earlier_calls() -> List<PhoneLog> {\n    from: PhoneLog\n    order: [PhoneLog.rang_at desc]\n    limit: 10\n}\n",
);

const ACTION: &str = "domains/support/actions/answer_call.jadpo";

/// The reply action: `body` before it returns `reply`.
fn answer(body: &str, reply: &str) -> String {
    format!(
        "use support/entities {{ PhoneLog }}\nuse support/queries {{ earlier_calls }}\nuse shared/providers/acme_voice\n\n/// Answers a call Acme Voice rings through, noting who rang.\naction answer_call(event: AcmeVoice.IncomingCall) -> AcmeVoice.CallReply {{\n    var earlier: List<PhoneLog> = earlier_calls()\n    var noted_call: PhoneLog = PhoneLog.create {{\n        set: {{\n            call_sid: event.call_sid\n            caller: event.from\n            rang_at: event.occurred_at\n            secret_note: \"\"\n        }}\n    }}\n{body}    return {reply}\n}}\n"
    )
}

/// Each problem of the help desk with PhoneLog's `create:` grants and
/// the reply action `action`, as `path rule: message`, but for unused
/// imports and declarations, migrations and records, which these
/// projects don't keep.
fn problems(create: &str, action: &str) -> Vec<String> {
    problems_in(&phone_log(create), action, &[])
}

/// [`problems`] with `log` as PhoneLog's file and `more` files besides.
fn problems_in(log: &str, action: &str, more: &[(&str, &str)]) -> Vec<String> {
    let mut files: Vec<(String, String)> = vec![
        ("app.jadpo".into(), APP.into()),
        ("config.jadpo".into(), CONFIG.into()),
        (
            "domains/support/entities/phone_log.jadpo".into(),
            log.into(),
        ),
        (EARLIER.0.into(), EARLIER.1.into()),
        (ACTION.into(), action.into()),
    ];
    files.extend(
        more.iter()
            .map(|(path, text)| (path.to_string(), text.to_string())),
    );
    files.extend(
        PACKAGE
            .iter()
            .map(|(path, text)| (format!("{HOME}/{path}"), text.to_string())),
    );
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    support::problems(Layout::Project, &files)
        .into_iter()
        .filter(|problem| {
            ![
                "import.unused",
                "layout.unused_declaration",
                "semantic.unused_variable",
            ]
            .iter()
            .any(|rule| problem.contains(&format!(" {rule}:")))
                && !problem.contains(" migration.")
                && !problem.contains(" record.")
        })
        .collect()
}

const SYSTEM: &str = "Access.system";

const HEARD: &str = "AcmeVoice.CallReply {\n        spoken: \"Thanks for calling.\"\n        hang_up: noted_call.caller != \"\"\n    }";

/// A reply made of what the provider sent, and of what the action set
/// on the record it created, is the provider's own.
#[test]
fn a_reply_of_what_the_provider_sent_checks() {
    assert_eq!(problems(SYSTEM, &answer("", HEARD)), Vec::<String>::new());
}

/// A reply carrying a field only `Access.system` may read is refused:
/// it goes to whoever sent the webhook.
#[test]
fn a_reply_carrying_what_only_the_system_reads_is_refused() {
    let carries = answer(
        "",
        "AcmeVoice.CallReply {\n        spoken: text.join(list.map(earlier, call -> call.secret_note), \" \")\n        hang_up: false\n    }",
    );
    assert_eq!(
        problems(SYSTEM, &carries),
        [
            "domains/support/actions/answer_call.jadpo policy.output_field_unproved: `answer_call`'s reply carries `PhoneLog.secret_note`, which not everyone may read"
        ]
    );
    // What decided the reply shows what it tested.
    let decided = answer(
        "    var mut said: Text = \"Thanks for calling.\"\n    for call in earlier {\n        if (call.secret_note == \"welcome\") {\n            said = \"Welcome back.\"\n        }\n    }\n",
        "AcmeVoice.CallReply {\n        spoken: said\n        hang_up: false\n    }",
    );
    assert_eq!(
        problems(SYSTEM, &decided),
        [
            "domains/support/actions/answer_call.jadpo policy.output_field_unproved: `answer_call`'s reply carries `PhoneLog.secret_note`, which not everyone may read"
        ]
    );
}

/// The reply action runs as the system, which a grant naming only
/// `Access.public` never lets in.
#[test]
fn a_reply_action_runs_as_the_system() {
    assert_eq!(
        problems("Access.public", &answer("", HEARD)),
        [
            "app.jadpo policy.system_grant_missing: `support.answer_call` runs as the system, which only `Access.public` lets create `PhoneLog`"
        ]
    );
}

const CALLS_SEEN: &str = "domains/support/actions/answer_call.jadpo policy.output_field_unproved: `answer_call`'s reply shows whether a `PhoneLog` is there, which not everyone may read";

/// A reply counting or testing for records only the system may read is
/// refused (chunk 7.9g): the action runs as the system and finds every
/// one, so whether one is there reaches whoever sent the webhook.
#[test]
fn a_reply_counting_or_testing_for_what_only_the_system_reads_is_refused() {
    let counted = answer(
        "",
        "AcmeVoice.CallReply {\n        spoken: text.from(list.count(earlier))\n        hang_up: false\n    }",
    );
    assert_eq!(problems(SYSTEM, &counted), [CALLS_SEEN]);
    let tested = answer(
        "    var mut said: Text = \"Thanks for calling.\"\n    if (list.count(earlier) > 0) {\n        said = \"Welcome back.\"\n    }\n",
        "AcmeVoice.CallReply {\n        spoken: said\n        hang_up: false\n    }",
    );
    assert_eq!(problems(SYSTEM, &tested), [CALLS_SEEN]);
    let looped = answer(
        "    var mut said: Text = \"Thanks for calling.\"\n    for call in earlier {\n        said = \"Welcome back.\"\n    }\n",
        "AcmeVoice.CallReply {\n        spoken: said\n        hang_up: false\n    }",
    );
    assert_eq!(problems(SYSTEM, &looped), [CALLS_SEEN]);
    // A record the action created is its own.
    let own = answer(
        "",
        "AcmeVoice.CallReply {\n        spoken: text.from(list.count([noted_call]))\n        hang_up: false\n    }",
    );
    assert_eq!(problems(SYSTEM, &own), Vec::<String>::new());
}

/// Counting records anyone at all may read shows nothing they couldn't.
#[test]
fn a_reply_counting_what_anyone_reads_checks() {
    let counted = answer(
        "",
        "AcmeVoice.CallReply {\n        spoken: text.from(list.count(earlier))\n        hang_up: false\n    }",
    );
    let log = phone_log(SYSTEM).replace(
        "read: [Access.system]",
        "read: [Access.public, Access.system]",
    );
    assert_eq!(problems_in(&log, &counted, &[]), Vec::<String>::new());
}

const BLOCKED: (&str, &str) = (
    "domains/support/failures/caller_blocked.jadpo",
    "/// The caller is one the desk doesn't take calls from.\ntype CallerBlocked = Failure {\n    kind: FailureKind.rejected\n    message: \"This caller is blocked.\"\n}\n",
);

/// The reply action, failing `CallerBlocked`, or `PhoneLogNotFound`
/// when it gets a `PhoneLog`, after `body`.
fn failing(body: &str) -> String {
    let fails = if body.contains("PhoneLog.get") {
        "PhoneLogNotFound"
    } else {
        "CallerBlocked"
    };
    format!(
        "use support/entities {{ PhoneLog, PhoneLogNotFound }}\nuse support/failures {{ CallerBlocked }}\nuse support/queries {{ earlier_calls }}\nuse shared/providers/acme_voice\n\n/// Answers a call Acme Voice rings through.\naction answer_call(event: AcmeVoice.IncomingCall) fails {fails} -> AcmeVoice.CallReply {{\n    var earlier: List<PhoneLog> = earlier_calls()\n{body}    return AcmeVoice.CallReply {{\n        spoken: \"Thanks for calling.\"\n        hang_up: false\n    }}\n}}\n"
    )
}

/// [`problems`] with the failure the reply action may raise.
fn failing_problems(body: &str) -> Vec<String> {
    problems_in(&phone_log(SYSTEM), &failing(body), &[BLOCKED])
}

/// A reply action's failure is answered to whoever sent the webhook too
/// (chunk 7.9g): whether it fails, and what its failure carries, shows
/// only what anyone at all may read.
#[test]
fn a_reply_action_failing_on_what_only_the_system_reads_is_refused() {
    assert_eq!(
        failing_problems(
            "    if (event.from == \"+447700900999\") {\n        reject CallerBlocked\n    }\n"
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        failing_problems(
            "    for call in earlier {\n        if (call.secret_note == \"blocked\") {\n            reject CallerBlocked\n        }\n    }\n"
        ),
        [
            "domains/support/actions/answer_call.jadpo policy.output_field_unproved: whether `answer_call` fails shows `PhoneLog.secret_note`, which not everyone may read",
            // The reply after a loop that may have failed shows that none did.
            CALLS_SEEN,
        ]
    );
    assert_eq!(
        failing_problems(
            "    if (list.count(earlier) > 3) {\n        reject CallerBlocked\n    }\n"
        ),
        [
            "domains/support/actions/answer_call.jadpo policy.output_field_unproved: whether `answer_call` fails shows whether a `PhoneLog` is there, which not everyone may read"
        ]
    );
    // A record found by what the provider sent may not be there.
    assert_eq!(
        failing_problems(
            "    var rang: PhoneLog = attempt PhoneLog.get(PhoneLog.id(event.account_id))\n"
        ),
        [
            "domains/support/actions/answer_call.jadpo policy.output_field_unproved: whether `answer_call` fails shows whether a `PhoneLog` is there, which not everyone may read"
        ]
    );
}

/// A failure's client fields go back to whoever sent the webhook too.
#[test]
fn a_reply_action_s_failure_carrying_what_only_the_system_reads_is_refused() {
    let told = (
        "domains/support/failures/caller_blocked.jadpo",
        "/// The caller is one the desk doesn't take calls from.\ntype CallerBlocked = Failure {\n    kind: FailureKind.rejected\n    message: \"This caller is blocked.\"\n    client: {\n        why: Text\n    }\n}\n",
    );
    let action = failing(
        "    if (event.from == \"+447700900999\") {\n        reject CallerBlocked {\n            why: text.join(list.map(earlier, call -> call.secret_note), \" \")\n        }\n    }\n",
    );
    assert_eq!(
        problems_in(&phone_log(SYSTEM), &action, &[told]),
        [
            "domains/support/actions/answer_call.jadpo policy.output_field_unproved: `answer_call`'s failure `CallerBlocked` carries `PhoneLog.secret_note`, which not everyone may read"
        ]
    );
}
