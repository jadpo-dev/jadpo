//! Routes in the checker (area 7): what a route's contract says it takes
//! and sends.

use crate::support;

use jadpo_db::{Database, Layout};

const MEMOS: &str = r#"type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

action keep_memo(input: AddMemo) -> Unit {
    assert input.text == input.text
}

"#;

/// Each problem `routes` gives beside [`MEMOS`]: its rule and what it
/// says to do next.
fn next_steps(routes: &str) -> Vec<String> {
    let db = Database::default();
    let source = format!("{MEMOS}{routes}");
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", source.as_str())]);
    jadpo_check::check(&db, project)
        .iter()
        .map(|d| {
            format!(
                "{}: {}",
                d.rule.id,
                d.next
                    .as_ref()
                    .map(|next| next.text.as_str())
                    .unwrap_or_default()
            )
        })
        .collect()
}

#[test]
fn a_route_without_output_is_told_what_to_send() {
    let add = "routes Memos {\n    prefix: /memos\n\n    POST / {\n        input: AddMemo\n        run: add_memo(input)\n    }\n}\n";
    assert_eq!(
        next_steps(add),
        ["route.output_required: Add `output: MemoView`"]
    );
    // An action that returns `Unit` sends nothing: the route answers 204,
    // never `output: Unit`, which a route can't send (Phase 6 review).
    let keep = "routes Memos {\n    prefix: /memos\n\n    POST / {\n        input: AddMemo\n        run: keep_memo(input)\n    }\n}\n";
    assert_eq!(
        next_steps(keep),
        ["route.output_required: Add `status: SuccessStatus.no_content`"]
    );
    let answered = "routes Memos {\n    prefix: /memos\n\n    POST / {\n        input: AddMemo\n        status: SuccessStatus.no_content\n        run: keep_memo(input)\n    }\n}\n";
    let found = next_steps(answered);
    assert!(
        !found.iter().any(|line| line.starts_with("route.")),
        "{found:?}"
    );
}

#[test]
fn a_placeholder_naming_a_missing_field_is_refused_at_the_placeholder() {
    // Chunk 6.6d: `{memo_id: Memo.nope}` names a field `Memo` doesn't
    // declare. It was dropped with its route and nothing said why.
    let entity = "entity Note {\n    id: Uuid\n    text: Memo\n\n    policy: {\n        read: [Access.public]\n    }\n}\n\naction show_note(note_id: Note.id) -> MemoView {\n    return MemoView {\n        text: \"\"\n    }\n}\n\n";
    let routes = |ty: &str| {
        format!(
            "{entity}routes Notes {{\n    prefix: /notes\n\n    POST /{{note_id: {ty}}} {{\n        authentication: RouteAuthentication.none\n        output: MemoView\n        run: show_note(note_id)\n    }}\n}}\n"
        )
    };
    assert_eq!(
        next_steps(&routes("Note.nope")),
        ["type.unknown_field: Name a field of `Note`"]
    );
    // A near miss is offered as the fix.
    assert_eq!(
        next_steps(&routes("Note.idd")),
        ["type.unknown_field: Use `Note.id`"]
    );
    let found = next_steps(&routes("Note.id"));
    assert!(
        !found.iter().any(|line| line.starts_with("type.")),
        "{found:?}"
    );
}
