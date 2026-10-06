//! Required half-open Instant page bounds: TIME262/273, nominal input-only slice.
use jadpo_core::analyze_sources;
use jadpo_syntax::{Declaration, Expression, SourceFile, Statement};

fn source(bounds: &str) -> String {
    format!(
        r#"
entity Event {{ id: Uuid identity bucket: Text occurs_at: Instant other_at: Instant }}
input Window {{ bucket: Event.bucket start: Event.occurs_at end: Event.occurs_at after: EventCursor optional page_size: Int }}
value EventCursor {{ occurs_at: Event.occurs_at id: Event.id }}
output EventView {{ id: Event.id occurs_at: Event.occurs_at bucket: Event.bucket }}
output EventPage {{ items: List<EventView> next: EventCursor? }}
action page_events(input: Window) -> EventPage {{
    return attempt query page Event -> EventPage {{
        where: bucket == input.bucket
        {bounds}
        order_by: occurs_at asc, id asc
        after: optional input.after
        limit: input.page_size
        project: EventView
        cursor: EventCursor(occurs_at, id)
    }}
}}
"#
    )
}
fn codes(text: &str) -> Vec<String> {
    let project = analyze_sources(vec![SourceFile::new(
        "page-range.jadpo".into(),
        text.into(),
    )])
    .unwrap();
    project
        .syntax
        .diagnostics()
        .chain(project.semantics.diagnostics.iter())
        .chain(project.typing.diagnostics.iter())
        .chain(project.failures.diagnostics.iter())
        .map(|d| d.code.to_string())
        .collect()
}
fn clean(text: &str) {
    assert!(codes(text).is_empty(), "{:?}\n{text}", codes(text));
}
#[test]
fn required_half_open_pair_checks_and_keeps_both_authored_orders() {
    for b in [
        "and: occurs_at >= input.start\n        and: occurs_at < input.end",
        "and: occurs_at < input.end\n        and: occurs_at >= input.start",
    ] {
        let text = source(b);
        clean(&text);
        let parsed = jadpo_syntax::parse(std::path::Path::new("page.jadpo"), &text);
        let action = parsed
            .file
            .declarations
            .iter()
            .find_map(|d| {
                if let Declaration::Callable(c) = d {
                    Some(c)
                } else {
                    None
                }
            })
            .unwrap();
        let Statement::Return(s) = &action.body.statements[0] else {
            panic!()
        };
        let Expression::Attempt(a) = &s.value else {
            panic!()
        };
        let Expression::Query(q) = a.value.as_ref() else {
            panic!()
        };
        let page = q.page.as_ref().unwrap();
        assert_eq!(page.predicates.len(), 3);
        assert_eq!(page.predicates[1].field.text, "occurs_at");
        assert_eq!(page.predicates[2].field.text, "occurs_at");
        assert_eq!(
            format!("{:?}", page.predicates[1].operator),
            if b.contains("occurs_at >= input.start\n") {
                "RequiredGreaterEqual"
            } else {
                "RequiredLess"
            }
        );
    }
}
#[test]
fn incomplete_duplicate_or_mixed_range_pairs_refuse() {
    for b in ["and: occurs_at >= input.start","and: occurs_at < input.end","and: occurs_at >= input.start\n and: occurs_at >= input.end","and: occurs_at < input.start\n and: occurs_at < input.end","and: occurs_at >= input.start\n and: other_at < input.end","and: occurs_at >= input.start\n and: occurs_at < input.end\n and: occurs_at == input.start","and: occurs_at >= input.start\n and: occurs_at < input.end\n and: occurs_at <= optional input.end"] {
        let c=codes(&source(b));assert!(c.iter().any(|c|c=="TYPE_QUERY_PAGE_RANGE_PAIR"),"{b}: {c:?}");
    }
}
#[test]
fn bounds_must_be_direct_required_input_members() {
    let base = source("and: occurs_at >= input.start\n and: occurs_at < input.end");
    for text in [
        base.replace("start: Event.occurs_at", "start: Event.occurs_at optional"),
        base.replace("start: Event.occurs_at", "start: Event.occurs_at?"),
        base.replace("input.start\n", "none\n"),
        base.replace("input.start\n", "(input.start)\n"),
        base.replace("input.start\n", "Event.occurs_at(input.start)\n"),
        base.replace("input.start\n", "clock.now\n"),
        base.replace("input.start\n", "principal.start\n"),
        base.replace(
            "    return attempt",
            "    var lower = input.start\n    return attempt",
        )
        .replace("input.start\n and:", "lower\n and:"),
    ] {
        let c = codes(&text);
        assert!(
            c.iter().any(|c| c == "TYPE_QUERY_PAGE_RANGE_INPUT"),
            "{c:?}\n{text}"
        );
    }
}
#[test]
fn required_bound_nominals_do_not_widen_to_representation_siblings() {
    let base = source("and: occurs_at >= input.start\n and: occurs_at < input.end");
    for kind in [
        "Event.other_at",
        "Instant",
        "Text",
        "Time",
        "CalendarDate",
        "Int",
    ] {
        let text = base.replace("start: Event.occurs_at", &format!("start: {kind}"));
        let c = codes(&text);
        assert!(!c.is_empty(), "accepted {kind}");
        assert!(
            c.iter()
                .any(|c| c == "TYPE_MISMATCH" || c == "TYPE_SIBLING_MISMATCH"),
            "{kind}: {c:?}"
        );
    }
}
#[test]
fn stored_nullable_or_noninstant_fields_refuse() {
    let base = source("and: occurs_at >= input.start\n and: occurs_at < input.end");
    for declaration in [
        "occurs_at: Instant?",
        "occurs_at: Text",
        "occurs_at: Time",
        "occurs_at: CalendarDate",
        "occurs_at: Int",
    ] {
        let text = base.replace(
            "occurs_at: Instant other_at",
            &format!("{declaration} other_at"),
        );
        let c = codes(&text);
        assert!(
            c.iter().any(|c| c == "TYPE_QUERY_PAGE_RANGE_FIELD"),
            "{declaration}: {c:?}"
        );
    }
}
#[test]
fn existing_equal_and_optional_ordered_predicates_remain_supported() {
    clean(
        &source("and: other_at <= optional input.end")
            .replace("end: Event.occurs_at", "end: Event.other_at optional"),
    );
    clean(&source("and: occurs_at == input.start"));
}
#[test]
fn local_input_alias_and_shadow_are_not_declaration_bounds() {
    let base = source("and: occurs_at >= input.start\n and: occurs_at < input.end");
    for (prefix, root) in [
        ("var alias = input", "alias"),
        ("var input = input", "input"),
    ] {
        let text = base
            .replace(
                "    return attempt",
                &format!("    {prefix}\n    return attempt"),
            )
            .replace(
                "occurs_at >= input.start",
                &format!("occurs_at >= {root}.start"),
            );
        assert!(
            codes(&text)
                .iter()
                .any(|c| c == "TYPE_QUERY_PAGE_RANGE_INPUT"),
            "{text}"
        );
    }
    // A shadow in a completed nested scope does not hide the original parameter.
    let text = base.replace(
        "    return attempt",
        "    if input.page_size == 1 { var input = input }\n    return attempt",
    );
    clean(&text);
}
#[test]
fn only_one_required_field_pair_is_admitted() {
    let b="and: occurs_at >= input.start\n and: occurs_at < input.end\n and: other_at >= input.other_start\n and: other_at < input.other_end";
    let text = source(b).replace(
        "after: EventCursor",
        "other_start: Event.other_at other_end: Event.other_at after: EventCursor",
    );
    assert!(codes(&text)
        .iter()
        .any(|c| c == "TYPE_QUERY_PAGE_RANGE_PAIR"));
}
#[test]
fn formatter_preserves_required_operators_and_authored_pair_order() {
    for b in [
        "and: occurs_at>=input.start\n and: occurs_at<input.end",
        "and: occurs_at<input.end\n and: occurs_at>=input.start",
    ] {
        let text = source(b);
        let formatted = jadpo_core::format_source(&text);
        clean(&formatted);
        assert!(formatted.contains("occurs_at >= input.start"));
        assert!(formatted.contains("occurs_at < input.end"));
        let meaning = |s: &str| {
            jadpo_syntax::lex(std::path::Path::new("p.jadpo"), s)
                .tokens
                .iter()
                .filter(|t| {
                    !matches!(
                        t.kind,
                        jadpo_syntax::TokenKind::Whitespace | jadpo_syntax::TokenKind::Eof
                    )
                })
                .map(|t| (t.kind, t.text(s).to_string()))
                .collect::<Vec<_>>()
        };
        assert_eq!(meaning(&text), meaning(&formatted));
    }
}
#[test]
fn emitted_range_parameters_cursor_offsets_and_equality_index_scope_are_exact() {
    let text = source("and: occurs_at >= input.start\n and: occurs_at < input.end");
    clean(&text);
    let p = analyze_sources(vec![SourceFile::new("page-range.jadpo".into(), text)]).unwrap();
    let t = jadpo_core::derive_target(std::path::Path::new("."), &p).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(
        &t.iter()
            .find(|a| a.relative_path == "persistence/entities.json")
            .unwrap()
            .contents,
    )
    .unwrap();
    let plan = &manifest["query_plans"][0];
    for (dialect, placeholder) in [("postgres", "$"), ("sqlite", "?")] {
        for phase in ["first", "after"] {
            let sql = plan[format!("{dialect}_{phase}_sql")].as_str().unwrap();
            assert!(sql.contains(&format!("\"bucket\" = {placeholder}1 AND \"occurs_at\" >= {placeholder}2 AND \"occurs_at\" < {placeholder}3")),"{sql}");
            assert!(sql.contains(&format!("LIMIT {placeholder}6")));
            assert!(!sql.contains("IS NULL OR \"occurs_at\""));
        }
    }
    assert_eq!(plan["index"], "event_page_bucket_occurs_at_id_idx");
    let app = &t
        .iter()
        .find(|a| a.relative_path == "target/app.ts")
        .unwrap()
        .contents;
    assert!(app.contains("{ value: input.start, kind: \"Instant\" }"));
    assert!(app.contains("{ value: input.end, kind: \"Instant\" }"));
    let subject: serde_json::Value = serde_json::from_str(
        &jadpo_core::derive_approval_subject(std::path::Path::new("."), &p, None, None, None)
            .unwrap(),
    )
    .unwrap();
    let serialized = subject.to_string();
    assert!(serialized.contains("RequiredGreaterEqual"));
    assert!(serialized.contains("RequiredLess"));
}
#[test]
fn coherently_rehashed_all_copy_range_operator_changes_refuse_fresh_subject() {
    use sha2::{Digest, Sha256};
    let text = source("and: occurs_at >= input.start\n and: occurs_at < input.end");
    let project = analyze_sources(vec![SourceFile::new("page-range.jadpo".into(), text)]).unwrap();
    let raw =
        jadpo_core::derive_approval_subject(std::path::Path::new("."), &project, None, None, None)
            .unwrap();
    assert!(jadpo_core::validate_approval_export(&raw, &raw).is_ok());
    for operator in ["RequiredGreaterEqual", "RequiredLess"] {
        let replaced = raw.replace(operator, "Equal");
        assert_ne!(replaced, raw);
        let mut forged: serde_json::Value = serde_json::from_str(&replaced).unwrap();
        let state = &mut forged["canonical"]["after"];
        state["behavior_digest"] = format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&state["facts"]).unwrap())
        )
        .into();
        state.as_object_mut().unwrap().remove("state_digest");
        state["state_digest"] = format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(state).unwrap())
        )
        .into();
        forged["subject_digest"] = format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&forged["canonical"]).unwrap())
        )
        .into();
        assert!(jadpo_core::validate_approval_export(
            &raw,
            &serde_json::to_string(&forged).unwrap()
        )
        .is_err());
    }
}
#[test]
fn secondary_or_reversed_bounds_never_admit_hidden_expressions() {
    for reverse in [false, true] {
        for rhs in [
            "clock.now",
            "current_principal.cutoff",
            "principal.cutoff",
            "config.cutoff",
            "Event.occurs_at(input.end)",
            "(input.end)",
        ] {
            let upper = format!("and: occurs_at < {rhs}");
            let clauses = if reverse {
                format!("{upper}\n and: occurs_at >= input.start")
            } else {
                format!("and: occurs_at >= input.start\n {upper}")
            };
            let text = source(&clauses);
            let c = codes(&text);
            assert!(
                c.iter().any(|c| c == "TYPE_QUERY_PAGE_RANGE_INPUT"),
                "{c:?}\n{text}"
            );
        }
    }
}

#[test]
fn range_diagnostics_have_precise_copy_repairs_and_causal_source_spans() {
    for (text, code, span_text) in [
        (
            source("and: occurs_at >= input.start"),
            "TYPE_QUERY_PAGE_RANGE_PAIR",
            "where:",
        ),
        (
            source("and: occurs_at >= input.start\n and: occurs_at < input.end")
                .replace("occurs_at: Instant", "occurs_at: Instant?"),
            "TYPE_QUERY_PAGE_RANGE_FIELD",
            "occurs_at",
        ),
        (
            source("and: occurs_at >= clock.now\n and: occurs_at < input.end"),
            "TYPE_QUERY_PAGE_RANGE_INPUT",
            "clock.now",
        ),
    ] {
        let project = analyze_sources(vec![SourceFile::new(
            "range-diagnostic.jadpo".into(),
            text.clone(),
        )])
        .unwrap();
        let diagnostic = project
            .typing
            .diagnostics
            .iter()
            .find(|d| d.code == code)
            .unwrap();
        let span = diagnostic.primary.as_ref().unwrap();
        assert_eq!(span.source, "range-diagnostic.jadpo");
        let observed = &text[span.start..span.end];
        if code == "TYPE_QUERY_PAGE_RANGE_PAIR" {
            assert!(observed.contains(span_text));
        } else {
            assert_eq!(observed, span_text);
        }
        let definition = jadpo_diagnostics::catalogue_definition(code);
        assert!(definition.authored_copy);
        assert!(!definition.reason.contains("compiler-enforced"));
        assert_ne!(
            definition.recommended_title,
            "Update the source to satisfy this rule"
        );
        assert_eq!(diagnostic.reason, definition.reason);
        assert_eq!(
            diagnostic.recommended_next_step.title,
            definition.recommended_title
        );
        assert!(
            diagnostic.recommended_next_step.edits.is_empty(),
            "No speculative automatic source repair"
        );
    }
}
