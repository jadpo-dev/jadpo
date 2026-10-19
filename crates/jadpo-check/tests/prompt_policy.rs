//! `prompts.render` with fields not every caller may read (area 9, area
//! 21; chunk 5.8c, from the slice the Phase 5 soundness review couldn't
//! cover): a value rendered into a prompt keeps its field's rules, so the
//! messages, or the text of one, sent back to a caller who may not read
//! the field are `policy.output_field_unproved`, whether the field is read
//! by one user only or never leaves the server.

use crate::support;

use jadpo_db::{Database, Layout};

const SOURCE: &str = "type Handle = Text {
    max_length: 40
}

type NoteText = Text {
    max_length: 200
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText
    verdict: NoteText {
        policy: {
            read: [reviewer_id]
        }
    }
    memo: NoteText {
        policy: FieldAccess.server_only
    }

    policy: {
        read: [owner_id, reviewer_id]
    }
}

type NoteNotFound = Failure {
    kind: FailureKind.not_found
}

type SentPrompt = Output {
    messages: List<ChatMessage>
}

type NoteSummary = Output {
    summary: Text
}

principal {
    user: {
        subject: User.handle
    }
}
";

const PROMPT_TYPE: &str = "/// Asks for a summary of a note.
type SummariseNote = Prompt {
    text: NoteText
    aside: NoteText
}
";

const PROMPT: &str = "# Instructions

Summarise the note in one line.

# Request

<text>
{text}
</text>

<aside>
{aside}
</aside>
";

fn rules(action: &str) -> Vec<String> {
    let db = Database::default();
    let source = format!("{SOURCE}\n{action}");
    let project = support::project_with_records(
        &db,
        Layout::Loose,
        &[
            ("main.jadpo", &source),
            ("summarise_note.jadpo", PROMPT_TYPE),
        ],
        &[("summarise_note.prompt", PROMPT)],
    );
    let mut found: Vec<String> = jadpo_check::check(&db, project)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect();
    found.dedup();
    found
}

fn messages(field: &str) -> String {
    format!(
        "action preview_summary(note_id: Note.id) fails NoteNotFound -> SentPrompt {{
    var note: Note = attempt Note.get(note_id)
    return SentPrompt {{
        messages: prompts.render(SummariseNote {{
            text: note.text
            aside: note.{field}
        }})
    }}
}}
"
    )
}

fn content(field: &str) -> String {
    format!(
        "action preview_text(note_id: Note.id) fails NoteNotFound -> NoteSummary {{
    var note: Note = attempt Note.get(note_id)
    var sent: List<ChatMessage> = prompts.render(SummariseNote {{
        text: note.text
        aside: note.{field}
    }})
    var last: ChatMessage? = list.last(sent)
    var shown: Text = match last {{
        some(message) -> message.content
        none -> \"\"
    }}
    return NoteSummary {{
        summary: shown
    }}
}}
"
    )
}

#[test]
fn a_field_rendered_into_a_prompt_keeps_its_rules() {
    assert_eq!(rules(&messages("text")), Vec::<String>::new());
    for field in ["verdict", "memo"] {
        assert_eq!(
            rules(&messages(field)),
            ["policy.output_field_unproved"],
            "{field} in the messages"
        );
        assert_eq!(
            rules(&content(field)),
            ["policy.output_field_unproved"],
            "{field} in a message's text"
        );
    }
}
