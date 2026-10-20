//! Shared by the CLI's snapshot tests.

pub mod fake_hq;
pub mod fake_provider;
pub mod fake_smtp;

/// A slow machine adds the run's duration to the footer (` in 0.1 s`);
/// snapshots compare the output without it. Rich output ends the footer
/// with a colour reset, so escape sequences after the time are kept and
/// looked past.
pub fn without_run_time(text: &str) -> String {
    text.split_inclusive('\n')
        .map(|line| {
            let (body, end) = line
                .strip_suffix('\n')
                .map_or((line, ""), |body| (body, "\n"));
            let (body, escapes) = trailing_escapes(body);
            match body.rsplit_once(" in ") {
                Some((before, time))
                    if time
                        .strip_suffix(" s")
                        .is_some_and(|secs| secs.parse::<f64>().is_ok()) =>
                {
                    format!("{before}{escapes}{end}")
                }
                _ => line.to_string(),
            }
        })
        .collect()
}

/// `text` without the SGR escape sequences (`\x1b[…m`) it ends with, and
/// those sequences.
fn trailing_escapes(text: &str) -> (&str, &str) {
    let mut body = text;
    while body.ends_with('m') {
        match body.rfind('\x1b') {
            Some(start)
                if body[start + 1..body.len() - 1]
                    .strip_prefix('[')
                    .is_some_and(|codes| codes.chars().all(|c| c.is_ascii_digit() || c == ';')) =>
            {
                body = &body[..start];
            }
            _ => break,
        }
    }
    (body, &text[body.len()..])
}

#[test]
fn the_run_time_goes_in_every_mode() {
    assert_eq!(
        without_run_time("tested: 2 passed, 2 pure tests in 0.1 s\n"),
        "tested: 2 passed, 2 pure tests\n"
    );
    assert_eq!(
        without_run_time("\x1b[2m · 2 pure tests in 12.3 s\x1b[0m\n"),
        "\x1b[2m · 2 pure tests\x1b[0m\n"
    );
    assert_eq!(without_run_time("kept in 3 boxes\n"), "kept in 3 boxes\n");
}
