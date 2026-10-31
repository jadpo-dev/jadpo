//! What a test writes once (area 13): each item of a route test's
//! `request { }` and each placeholder in its `params:`, and each of a
//! test's settings, so there is never a question of which counts.

use jadpo_db::Layout;

use crate::support;

const CODE: &str = r#"type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action tag_memo(memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST /{memo_id: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(memo_id, input)
    }
}

policy {
    actions: {
        tag_memo: [Access.public]
    }
}

test "a memo is tagged" {
    clock: "2026-01-15T12:00:00Z"

    var response: Response = request {
        method: HttpMethod.post
        path: "/memos/{memo_id}"
        params: {
            memo_id: "0d9c1f1e-3b8a-4d6e-9f2a-6c5b4a3e2d1f"
        }
        input: AddMemo {
            text: "Call the plumber"
        }
    }
    assert response.status == 200
}
"#;

fn problems(code: &str) -> Vec<String> {
    support::problems(Layout::Loose, &[("main.jadpo", code)])
}

fn has(found: &[String], rule: &str, words: &str) -> bool {
    found
        .iter()
        .any(|problem| problem.contains(&format!("{rule}:")) && problem.contains(words))
}

#[test]
fn a_request_gives_each_item_once() {
    let found = problems(CODE);
    assert_eq!(found, Vec::<String>::new(), "{found:?}");
    for (line, words) in [
        (
            "        method: HttpMethod.post\n",
            "`method:` is given twice",
        ),
        (
            "        path: \"/memos/{memo_id}\"\n",
            "`path:` is given twice",
        ),
        (
            "        params: {\n            memo_id: \"0d9c1f1e-3b8a-4d6e-9f2a-6c5b4a3e2d1f\"\n        }\n",
            "`params:` is given twice",
        ),
        (
            "            memo_id: \"0d9c1f1e-3b8a-4d6e-9f2a-6c5b4a3e2d1f\"\n",
            "`memo_id` is given twice in `params:`",
        ),
    ] {
        let code = CODE.replacen(line, &format!("{line}{line}"), 1);
        let found = problems(&code);
        assert!(
            has(&found, "semantic.request_item_duplicate", words),
            "{line}: {found:?}"
        );
        assert_eq!(found.len(), 1, "{line}: {found:?}");
    }
}

#[test]
fn a_test_gives_each_setting_once() {
    let line = "    clock: \"2026-01-15T12:00:00Z\"\n";
    let code = CODE.replacen(
        line,
        &format!("{line}    clock: \"2026-01-16T12:00:00Z\"\n"),
        1,
    );
    let found = problems(&code);
    assert!(
        has(
            &found,
            "semantic.test_setting_duplicate",
            "`clock:` is given twice"
        ),
        "{found:?}"
    );
    assert_eq!(found.len(), 1, "{found:?}");
}
